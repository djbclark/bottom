//! Time-weighted sliding averages. Only numeric samples are retained: process
//! commands, paths, states, totals and other metadata always come from the latest harvest.
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use rustc_hash::FxHashMap;

use crate::{
    collection::{Data, processes::Pid},
    widgets::DiskWidgetData,
};

#[derive(Clone, Debug)]
struct Interval<const N: usize> {
    start: Instant,
    end: Instant,
    values: [f64; N],
}

#[derive(Clone, Debug)]
struct Window<const N: usize> {
    last: Instant,
    samples: VecDeque<Interval<N>>,
    sums: [f64; N],
    weights: [f64; N],
}

impl<const N: usize> Window<N> {
    fn new(now: Instant) -> Self {
        Self {
            last: now,
            samples: VecDeque::new(),
            sums: [0.0; N],
            weights: [0.0; N],
        }
    }

    fn accumulate(&mut self, values: &[f64; N], seconds: f64) {
        for (i, value) in values.iter().enumerate() {
            if value.is_finite() {
                self.sums[i] += value * seconds;
                self.weights[i] += seconds;
            }
        }
    }

    fn update(&mut self, now: Instant, duration: Duration, values: [f64; N]) -> [f64; N] {
        if now <= self.last {
            return values;
        }
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.accumulate(&values, elapsed);
        // Consecutive equal measurements share one interval (important for idle processes).
        if let Some(last) = self.samples.back_mut()
            && last
                .values
                .iter()
                .zip(values)
                .all(|(a, b)| *a == b || (a.is_nan() && b.is_nan()))
        {
            last.end = now;
        } else {
            self.samples.push_back(Interval {
                start: self.last,
                end: now,
                values,
            });
        }
        self.last = now;
        // Work in elapsed durations, avoiding Instant subtraction overflow for long windows.
        while let Some(first) = self.samples.front() {
            if now.duration_since(first.end) >= duration {
                let first = self.samples.pop_front().expect("front exists");
                self.accumulate(
                    &first.values,
                    -first.end.duration_since(first.start).as_secs_f64(),
                );
            } else {
                break;
            }
        }
        if let Some(first) = self.samples.front_mut() {
            let age = now.duration_since(first.start);
            if age > duration {
                let excess = age - duration;
                first.start += excess;
                let values = first.values;
                self.accumulate(&values, -excess.as_secs_f64());
            }
        }
        std::array::from_fn(|i| {
            if values[i].is_finite() && self.weights[i] > 0.0 {
                self.sums[i] / self.weights[i]
            } else {
                values[i]
            }
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ResourceAverages {
    previous: Option<Instant>,
    duration: Duration,
    scalar: FxHashMap<String, Window<1>>,
    sensor_names: Vec<String>,
    #[cfg(feature = "gpu")]
    gpu_names: Vec<String>,
    processes: FxHashMap<(Pid, u64), Window<10>>,
}

impl ResourceAverages {
    fn scalar(&mut self, key: String, now: Instant, value: f64) -> f64 {
        let previous = self.previous;
        let window = self.scalar.entry(key).or_insert_with(|| Window::new(now));
        // Missing collection is unknown, not zero and not an extension of the previous value.
        if Some(window.last) != previous {
            *window = Window::new(now);
        }
        window.update(now, self.duration, [value])[0]
    }

    pub(super) fn apply(&mut self, data: &mut Data, duration: Duration) {
        let now = data.collection_time;
        if self.duration != duration {
            *self = Self {
                duration,
                ..Default::default()
            };
        }
        if duration.is_zero() {
            return;
        }
        if let Some(cpu) = &mut data.cpu {
            for value in cpu {
                value.usage = self.scalar(
                    format!("cpu:{:?}", value.data_type),
                    now,
                    value.usage.into(),
                ) as f32;
            }
        }
        macro_rules! memory {
            ($field:ident) => {
                if let Some(value) = &mut data.$field {
                    value.used_bytes = self
                        .scalar(stringify!($field).into(), now, value.used_bytes as f64)
                        .round() as u64;
                }
            };
        }
        memory!(memory);
        memory!(swap);
        #[cfg(not(target_os = "windows"))]
        {
            memory!(cache);
        }
        #[cfg(feature = "zfs")]
        {
            memory!(arc);
        }
        #[cfg(feature = "gpu")]
        if let Some(gpus) = &mut data.gpu {
            let names: Vec<_> = gpus.iter().map(|(name, _)| name.clone()).collect();
            if self.gpu_names != names {
                self.scalar.retain(|key, _| !key.starts_with("gpu:"));
                self.gpu_names = names;
            }
            for (index, (name, memory)) in gpus.iter_mut().enumerate() {
                memory.used_bytes = self
                    .scalar(format!("gpu:{index}:{name}"), now, memory.used_bytes as f64)
                    .round() as u64;
            }
        }
        if let Some(network) = &mut data.network {
            macro_rules! rate {
                ($field:ident) => {
                    network.$field = self
                        .scalar(
                            concat!("network:", stringify!($field)).into(),
                            now,
                            network.$field as f64,
                        )
                        .round() as u64;
                };
            }
            rate!(rx);
            rate!(tx);
            rate!(rx_packets);
            rate!(tx_packets);
        }
        if let Some(sensors) = &mut data.temperature_sensors {
            let names: Vec<_> = sensors.iter().map(|sensor| sensor.name.clone()).collect();
            if self.sensor_names != names {
                self.scalar.retain(|key, _| !key.starts_with("temp:"));
                self.sensor_names = names;
            }
            for (index, sensor) in sensors.iter_mut().enumerate() {
                if let Some(value) = &mut sensor.temperature {
                    *value = self.scalar(
                        format!("temp:{index}:{}", sensor.name),
                        now,
                        (*value).into(),
                    ) as f32;
                }
            }
        }
        #[cfg(feature = "battery")]
        if let Some(batteries) = &mut data.list_of_batteries {
            for (i, battery) in batteries.iter_mut().enumerate() {
                battery.charge_percent =
                    self.scalar(format!("battery:{i}:charge"), now, battery.charge_percent);
                battery.power_consumption =
                    self.scalar(format!("battery:{i}:power"), now, battery.power_consumption);
            }
        }
        if let Some(processes) = &mut data.list_of_processes {
            for process in processes {
                let window = self
                    .processes
                    .entry((process.pid, process.start_time))
                    .or_insert_with(|| Window::new(now));
                if Some(window.last) != self.previous {
                    *window = Window::new(now);
                }
                #[allow(unused_mut)]
                let mut values = [
                    process.cpu_usage_percent.into(),
                    process.mem_usage_percent.into(),
                    process.mem_usage as f64,
                    process.virtual_mem as f64,
                    process.swap_bytes.map_or(f64::NAN, |v| v as f64),
                    process.read_per_sec as f64,
                    process.write_per_sec as f64,
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                ];
                #[cfg(feature = "gpu")]
                {
                    values[7] = process.gpu_mem as f64;
                    values[8] = process.gpu_mem_percent.into();
                    values[9] = process.gpu_util.into();
                }
                let values = window.update(now, duration, values);
                process.cpu_usage_percent = values[0] as f32;
                process.mem_usage_percent = values[1] as f32;
                process.mem_usage = values[2].round() as u64;
                process.virtual_mem = values[3].round() as u64;
                process.swap_bytes = process.swap_bytes.map(|_| values[4].round() as u64);
                process.read_per_sec = values[5].round() as u64;
                process.write_per_sec = values[6].round() as u64;
                #[cfg(feature = "gpu")]
                {
                    process.gpu_mem = values[7].round() as u64;
                    process.gpu_mem_percent = values[8] as f32;
                    process.gpu_util = values[9].round() as u32;
                }
            }
        }
    }

    pub(super) fn apply_disks(
        &mut self, disks: &mut [DiskWidgetData], now: Instant, duration: Duration,
    ) {
        if duration.is_zero() {
            return;
        }
        for disk in disks {
            macro_rules! gauge {
                ($field:ident) => {
                    if let Some(value) = &mut disk.$field {
                        *value = self
                            .scalar(
                                format!(
                                    "disk:{}:{}:{}",
                                    disk.name,
                                    disk.mount_point,
                                    stringify!($field)
                                ),
                                now,
                                *value as f64,
                            )
                            .round() as u64;
                    }
                };
            }
            gauge!(used_bytes);
            gauge!(free_bytes);
            gauge!(io_read_rate_bytes);
            gauge!(io_write_rate_bytes);
        }
    }

    pub(super) fn finish(&mut self, now: Instant) {
        self.scalar.retain(|_, window| window.last == now);
        self.processes.retain(|_, window| window.last == now);
        self.previous = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_intervals_and_clips_boundary() {
        let start = Instant::now();
        let mut window = Window::new(start);
        let span = Duration::from_secs(5);
        assert_eq!(
            window.update(start + Duration::from_secs(2), span, [10.0]),
            [10.0]
        );
        // Two seconds at 10, four at 20. Only one second at 10 remains.
        assert_eq!(
            window.update(start + Duration::from_secs(6), span, [20.0]),
            [18.0]
        );
        assert_eq!(
            window.update(start + Duration::from_secs(20), span, [0.0]),
            [0.0]
        );
        assert_eq!(window.samples.len(), 1);
    }

    #[test]
    fn warmup_and_zero_elapsed_have_no_invented_history() {
        let start = Instant::now();
        let mut window = Window::new(start);
        assert_eq!(
            window.update(start, Duration::from_secs(300), [95.0]),
            [95.0]
        );
        assert!(window.samples.is_empty());
        assert_eq!(
            window.update(
                start + Duration::from_secs(1),
                Duration::from_secs(300),
                [10.0]
            ),
            [10.0]
        );
    }

    #[test]
    fn missing_values_do_not_dilute_valid_measurements() {
        let start = Instant::now();
        let mut window = Window::new(start);
        let span = Duration::from_secs(5);
        window.update(start + Duration::from_secs(1), span, [10.0]);
        assert!(window.update(start + Duration::from_secs(2), span, [f64::NAN])[0].is_nan());
        assert_eq!(
            window.update(start + Duration::from_secs(3), span, [20.0]),
            [15.0]
        );
    }
    fn process_data(now: Instant, start_time: u64, cpu: f32) -> Data {
        Data {
            collection_time: now,
            list_of_processes: Some(vec![crate::collection::processes::ProcessHarvest {
                pid: 42,
                start_time,
                cpu_usage_percent: cpu,
                total_read: 1234,
                mem_usage: 100,
                ..Default::default()
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn process_identity_gc_and_disabled_mode() {
        let now = Instant::now();
        let span = Duration::from_secs(300);
        let mut averages = ResourceAverages::default();
        for (second, cpu, expected) in [(0, 0.0, 0.0), (1, 90.0, 90.0), (3, 0.0, 30.0)] {
            let time = now + Duration::from_secs(second);
            let mut data = process_data(time, 1, cpu);
            averages.apply(&mut data, span);
            let process = &data.list_of_processes.as_ref().unwrap()[0];
            assert_eq!(process.cpu_usage_percent, expected);
            assert_eq!(process.total_read, 1234);
            averages.finish(time);
        }
        let time = now + Duration::from_secs(4);
        let mut reused = process_data(time, 2, 10.0);
        averages.apply(&mut reused, span);
        averages.finish(time);
        assert_eq!(reused.list_of_processes.unwrap()[0].cpu_usage_percent, 10.0);
        assert_eq!(averages.processes.len(), 1);
        let mut missing = Data {
            collection_time: now + Duration::from_secs(5),
            ..Default::default()
        };
        averages.apply(&mut missing, span);
        averages.finish(missing.collection_time);
        assert!(averages.processes.is_empty());
        let mut disabled = process_data(now + Duration::from_secs(6), 2, 70.0);
        averages.apply(&mut disabled, Duration::ZERO);
        assert!(averages.processes.is_empty());
        assert_eq!(
            disabled.list_of_processes.unwrap()[0].cpu_usage_percent,
            70.0
        );
    }

    #[test]
    fn idle_process_samples_compress_even_with_optional_values() {
        let now = Instant::now();
        let mut averages = ResourceAverages::default();
        for second in 0..20 {
            let time = now + Duration::from_secs(second);
            let mut data = process_data(time, 1, 0.0);
            averages.apply(&mut data, Duration::from_secs(300));
            averages.finish(time);
        }
        assert_eq!(averages.processes.values().next().unwrap().samples.len(), 1);
    }
    #[test]
    fn duplicate_sensor_names_keep_independent_histories() {
        let now = Instant::now();
        let mut averages = ResourceAverages::default();
        let span = Duration::from_secs(300);
        for (second, values, expected) in [
            (0, [0.0, 100.0], [0.0, 100.0]),
            (1, [10.0, 90.0], [10.0, 90.0]),
            (2, [30.0, 70.0], [20.0, 80.0]),
        ] {
            let time = now + Duration::from_secs(second);
            let mut data = Data {
                collection_time: time,
                temperature_sensors: Some(
                    values
                        .into_iter()
                        .map(
                            |temperature| crate::collection::temperature::TempSensorData {
                                name: "identical sensor".to_string(),
                                temperature: Some(temperature),
                            },
                        )
                        .collect(),
                ),
                ..Default::default()
            };
            averages.apply(&mut data, span);
            averages.finish(time);
            let temperatures: Vec<_> = data
                .temperature_sensors
                .unwrap()
                .into_iter()
                .map(|sensor| sensor.temperature.unwrap())
                .collect();
            assert_eq!(temperatures, expected);
        }
        let time = now + Duration::from_secs(3);
        let mut changed = Data {
            collection_time: time,
            temperature_sensors: Some(vec![crate::collection::temperature::TempSensorData {
                name: "identical sensor".to_string(),
                temperature: Some(50.0),
            }]),
            ..Default::default()
        };
        averages.apply(&mut changed, span);
        assert_eq!(
            changed.temperature_sensors.unwrap()[0].temperature,
            Some(50.0)
        );
    }
}
