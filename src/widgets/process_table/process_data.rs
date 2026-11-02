use std::{
    borrow::Cow,
    cmp::{Ordering, max},
    fmt::Display,
    num::NonZeroU16,
    ops::Add,
    sync::Arc,
    time::Duration,
};

use concat_string::concat_string;
use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::Row,
};
use unicode_ellipsis::truncate_str;
use unicode_width::UnicodeWidthStr;

use super::process_columns::ProcColumn;
#[cfg(target_os = "linux")]
use crate::collection::processes::ProcessType;
use crate::{
    canvas::{
        Painter,
        components::data_table::{DataTableColumn, DataToCell},
    },
    collection::processes::{Bytes, Pid, ProcessHarvest, SplitCommand},
    dec_bytes_per_second_string,
    utils::{
        data_units::{GIBI_LIMIT, GIGA_LIMIT, get_binary_bytes, get_decimal_bytes},
        strings::truncate_path,
    },
};

/// Separates the parts of a split command.
const SPLIT_SEPARATOR: &str = " ┊ ";

/// Shown for a working directory that could not be read.
const UNKNOWN_CWD: &str = "?";

/// The options of a split command keep this many cells (or all of them, if
/// shorter) before the working directory is given the rest.
const MIN_ARGS_WIDTH: usize = 10;

/// What the name/command column shows for a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdKind {
    /// The process name.
    Name,
    /// The full command.
    Command,
    /// The full command, split into name, working directory and options
    /// (`processes.split_command`).
    SplitCommand,
}

#[derive(Clone, Debug)]
enum IdType {
    Name(String),
    Command(String),
    Split(SplitCommand),
}

#[derive(Clone, Debug)]
pub struct Id {
    id_type: IdType,
    prefix: Option<String>,
}

impl From<&'static str> for Id {
    fn from(s: &'static str) -> Self {
        Id {
            id_type: IdType::Name(s.to_string()),
            prefix: None,
        }
    }
}

/// A split command as one line of plain text, with the parts separated by
/// [`SPLIT_SEPARATOR`].
fn split_text(split: &SplitCommand) -> String {
    let cwd = split.cwd.as_deref().unwrap_or(UNKNOWN_CWD);
    if split.args.is_empty() {
        concat_string!(split.name, SPLIT_SEPARATOR, cwd)
    } else {
        concat_string!(
            split.name,
            SPLIT_SEPARATOR,
            cwd,
            SPLIT_SEPARATOR,
            split.args
        )
    }
}

/// Cell widths given to each part of a split command. `None` means the part,
/// and the separator before it, is not shown.
#[derive(Debug, PartialEq, Eq)]
struct SplitWidths {
    name: usize,
    cwd: Option<usize>,
    args: Option<usize>,
}

/// Shares `width` cells between the parts of a split command whose full
/// widths are given. The name comes first; then the options keep up to
/// [`MIN_ARGS_WIDTH`] cells; then the working directory gets what it needs of
/// the rest; then the options get whatever is left.
fn split_widths(name: usize, cwd: usize, args: usize, width: usize) -> SplitWidths {
    let separator = SPLIT_SEPARATOR.width();

    // Room for the name, a separator and at least one cell of the directory?
    if name + separator >= width {
        return SplitWidths {
            name: name.min(width),
            cwd: None,
            args: None,
        };
    }
    let rest = width - name - separator;

    // Room for one cell of directory, a separator and one cell of options?
    if args == 0 || rest < 1 + separator + 1 {
        return SplitWidths {
            name,
            cwd: Some(cwd.min(rest)),
            args: None,
        };
    }

    let args_kept = args.min(MIN_ARGS_WIDTH).min(rest - 1 - separator);
    let cwd = cwd.min(rest - separator - args_kept);
    let args = args.min(rest - separator - cwd);
    SplitWidths {
        name,
        cwd: Some(cwd),
        args: Some(args),
    }
}

/// Draws a split command (after an optional `prefix`, such as a tree branch)
/// in `width` cells: the name, the working directory cut in the middle and the
/// options cut at the end, separated by [`SPLIT_SEPARATOR`] in
/// `separator_style`.
fn split_line(
    prefix: Option<&str>, split: &SplitCommand, width: usize, separator_style: Style,
    unknown_style: Style,
) -> Line<'static> {
    let mut spans = Vec::with_capacity(6);
    let mut width = width;

    if let Some(prefix) = prefix {
        let prefix = truncate_str(prefix, width);
        width = width.saturating_sub(prefix.width());
        spans.push(Span::raw(prefix.into_owned()));
    }

    let cwd_text = split.cwd.as_deref().unwrap_or(UNKNOWN_CWD);
    let widths = split_widths(
        split.name.width(),
        cwd_text.width(),
        split.args.width(),
        width,
    );

    spans.push(Span::raw(
        truncate_str(&split.name, widths.name).into_owned(),
    ));
    if let Some(cwd_width) = widths.cwd {
        spans.push(Span::styled(SPLIT_SEPARATOR, separator_style));
        spans.push(match &split.cwd {
            Some(cwd) => Span::raw(truncate_path(cwd, cwd_width).into_owned()),
            None => Span::styled(UNKNOWN_CWD, unknown_style),
        });
    }
    if let Some(args_width) = widths.args {
        spans.push(Span::styled(SPLIT_SEPARATOR, separator_style));
        spans.push(Span::raw(
            truncate_str(&split.args, args_width).into_owned(),
        ));
    }

    Line::from(spans)
}

impl Id {
    /// Returns the ID as a lowercase [`String`], with no prefix. This is
    /// primarily useful for cases like sorting where we treat everything as
    /// the same case (e.g. `Discord` comes before `dkms`).
    pub fn to_lowercase(&self) -> String {
        match &self.id_type {
            IdType::Name(name) => name.to_lowercase(),
            IdType::Command(cmd) => cmd.to_lowercase(),
            IdType::Split(split) => split_text(split).to_lowercase(),
        }
    }

    /// Return the ID as a borrowed [`str`] with no prefix. For a split
    /// command, this is its name.
    pub fn as_str(&self) -> &str {
        match &self.id_type {
            IdType::Name(name) => name.as_str(),
            IdType::Command(cmd) => cmd.as_str(),
            IdType::Split(split) => split.name.as_str(),
        }
    }

    /// Returns the ID as a [`String`] with prefix.
    pub fn to_prefixed_string(&self) -> String {
        let prefix = self.prefix.as_deref().unwrap_or_default();
        match &self.id_type {
            IdType::Name(name) => concat_string!(prefix, name),
            IdType::Command(cmd) => concat_string!(prefix, cmd),
            IdType::Split(split) => concat_string!(prefix, split_text(split)),
        }
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(PartialEq, Clone, Debug, Copy)]
pub enum MemUsage {
    Percent(f32),
    Bytes(u64),
}

impl Add for MemUsage {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (MemUsage::Percent(a), MemUsage::Percent(b)) => MemUsage::Percent(a + b),
            (MemUsage::Bytes(a), MemUsage::Bytes(b)) => MemUsage::Bytes(a + b),
            (MemUsage::Bytes(_), MemUsage::Percent(_))
            | (MemUsage::Percent(_), MemUsage::Bytes(_)) => {
                unreachable!("trying to add together two different memory usage types!")
            }
        }
    }
}

impl PartialOrd for MemUsage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (MemUsage::Percent(a), MemUsage::Percent(b)) => a.partial_cmp(b),
            (MemUsage::Bytes(a), MemUsage::Bytes(b)) => a.partial_cmp(b),
            _ => unreachable!(),
        }
    }
}

impl Display for MemUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemUsage::Percent(percent) => f.write_fmt(format_args!("{percent:.1}%")),
            MemUsage::Bytes(bytes) => f.write_str(&binary_byte_string(*bytes)),
        }
    }
}

trait DurationExt {
    fn num_days(&self) -> u64;
    fn num_hours(&self) -> u64;
    fn num_minutes(&self) -> u64;
}

const SECS_PER_DAY: u64 = SECS_PER_HOUR * 24;
const SECS_PER_HOUR: u64 = SECS_PER_MINUTE * 60;
const SECS_PER_MINUTE: u64 = 60;

impl DurationExt for Duration {
    /// Number of full days in this duration.
    #[inline]
    fn num_days(&self) -> u64 {
        self.as_secs() / SECS_PER_DAY
    }

    /// Number of full hours in this duration.
    #[inline]
    fn num_hours(&self) -> u64 {
        self.as_secs() / SECS_PER_HOUR
    }

    /// Number of full minutes in this duration.
    #[inline]
    fn num_minutes(&self) -> u64 {
        self.as_secs() / SECS_PER_MINUTE
    }
}

fn format_time(dur: Duration) -> String {
    if dur.num_days() > 0 {
        format!(
            "{}d {}h {}m",
            dur.num_days(),
            dur.num_hours() % 24,
            dur.num_minutes() % 60
        )
    } else if dur.num_hours() > 0 {
        format!(
            "{}h {}m {}s",
            dur.num_hours(),
            dur.num_minutes() % 60,
            dur.as_secs() % 60
        )
    } else if dur.num_minutes() > 0 {
        format!(
            "{}m {}.{:02}s",
            dur.num_minutes(),
            dur.as_secs() % 60,
            dur.as_millis() % 1000 / 10
        )
    } else {
        format!("{}.{:03}s", dur.as_secs(), dur.as_millis() % 1000)
    }
}

/// Returns a string given a value that is converted to the closest binary
/// variant. If the value is greater than a gibibyte, then it will return a
/// decimal place.
#[inline]
fn binary_byte_string(value: u64) -> String {
    let converted_values = get_binary_bytes(value);
    if value >= GIBI_LIMIT {
        format!("{:.1}{}", converted_values.0, converted_values.1)
    } else {
        format!("{:.0}{}", converted_values.0, converted_values.1)
    }
}

/// Returns a string given a value that is converted to the closest SI-variant.
/// If the value is greater than a giga-X, then it will return a decimal place.
fn dec_bytes_string(value: u64) -> String {
    let converted_values = get_decimal_bytes(value);
    if value >= GIGA_LIMIT {
        format!("{:.1}{}", converted_values.0, converted_values.1)
    } else {
        format!("{:.0}{}", converted_values.0, converted_values.1)
    }
}

#[derive(Clone)]
pub struct ProcWidgetData {
    pub pid: Pid,
    #[allow(dead_code)]
    pub ppid: Option<Pid>,
    pub id: Id,
    pub cpu_usage_percent: f32,
    pub mem_usage: MemUsage,
    pub virtual_mem: u64,
    pub swap_bytes: Option<Bytes>,
    pub rps: u64,
    pub wps: u64,
    pub total_read: u64,
    pub total_write: u64,
    pub process_state: &'static str,
    pub process_char: char,
    pub user: Option<Arc<str>>,
    pub num_similar: u64,
    /// Whether this is one process of an expanded group (grouped mode with
    /// `split_command`), shown under the group's row.
    pub is_group_member: bool,
    pub disabled: bool,
    pub time: Duration,
    #[cfg(feature = "gpu")]
    pub gpu_mem_usage: MemUsage,
    #[cfg(feature = "gpu")]
    pub gpu_usage: u32,
    /// The process "type". Used to colour things.
    #[cfg(target_os = "linux")]
    pub process_type: ProcessType,
    #[cfg(unix)]
    pub nice: i32,
    pub priority: i32,
}

impl ProcWidgetData {
    pub fn from_data(process: &ProcessHarvest, id_kind: IdKind, is_mem_percent: bool) -> Self {
        let id_type = match (id_kind, &process.split_command) {
            (IdKind::Name, _) => IdType::Name(process.name.clone()),
            (IdKind::SplitCommand, Some(split)) => IdType::Split(split.clone()),
            (IdKind::Command | IdKind::SplitCommand, _) => IdType::Command(process.command.clone()),
        };
        let id = Id {
            id_type,
            prefix: None,
        };

        let mem_usage = if is_mem_percent {
            MemUsage::Percent(process.mem_usage_percent)
        } else {
            MemUsage::Bytes(process.mem_usage)
        };

        Self {
            pid: process.pid,
            ppid: process.parent_pid,
            id,
            cpu_usage_percent: process.cpu_usage_percent,
            mem_usage,
            virtual_mem: process.virtual_mem,
            swap_bytes: process.swap_bytes,
            rps: process.read_per_sec,
            wps: process.write_per_sec,
            total_read: process.total_read,
            total_write: process.total_write,
            process_state: process.process_state.0,
            process_char: process.process_state.1,
            user: process.user.clone(),
            num_similar: 1,
            is_group_member: false,
            disabled: false,
            time: process.time,
            #[cfg(feature = "gpu")]
            gpu_mem_usage: if is_mem_percent {
                MemUsage::Percent(process.gpu_mem_percent)
            } else {
                MemUsage::Bytes(process.gpu_mem)
            },
            #[cfg(feature = "gpu")]
            gpu_usage: process.gpu_util,
            #[cfg(target_os = "linux")]
            process_type: process.process_type,
            #[cfg(unix)]
            nice: process.nice,
            priority: process.priority,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn prefix(mut self, prefix: Option<String>) -> Self {
        self.id.prefix = prefix;
        self
    }

    /// Makes this the row of a group of processes with the same split command
    /// name, showing just that name after a `+` (collapsed) or `-` (expanded).
    pub fn group_head(mut self, expanded: bool) -> Self {
        self.id = Id {
            id_type: IdType::Name(self.id.as_str().to_string()),
            prefix: Some(if expanded { "- " } else { "+ " }.to_string()),
        };
        self
    }

    /// Makes this one process of an expanded group, drawn with `prefix` (its
    /// tree branch).
    pub fn group_member(mut self, prefix: &str) -> Self {
        self.is_group_member = true;
        self.id.prefix = Some(prefix.to_string());
        self
    }

    pub fn add(&mut self, other: &Self) {
        self.cpu_usage_percent += other.cpu_usage_percent;
        self.mem_usage = self.mem_usage + other.mem_usage;
        self.rps += other.rps;
        self.wps += other.wps;
        self.total_read += other.total_read;
        self.total_write += other.total_write;
        self.time = self.time.max(other.time);
        self.swap_bytes = match (self.swap_bytes, other.swap_bytes) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        #[cfg(feature = "gpu")]
        {
            self.gpu_mem_usage = self.gpu_mem_usage + other.gpu_mem_usage;
            self.gpu_usage += other.gpu_usage;
        }
    }

    fn to_string(&self, column: &ProcColumn) -> String {
        match column {
            &ProcColumn::Priority => self.priority.to_string(),
            #[cfg(unix)]
            ProcColumn::Nice => self.nice.to_string(),
            ProcColumn::CpuPercent => format!("{:.1}%", self.cpu_usage_percent),
            ProcColumn::MemValue | ProcColumn::MemPercent => self.mem_usage.to_string(),
            ProcColumn::VirtualMem => binary_byte_string(self.virtual_mem),
            ProcColumn::Swap => self
                .swap_bytes
                .map(binary_byte_string)
                .unwrap_or_else(|| "N/A".to_string()),
            ProcColumn::Pid => self.pid.to_string(),
            ProcColumn::Count => self.num_similar.to_string(),
            ProcColumn::Name | ProcColumn::Command => self.id.to_prefixed_string(),
            ProcColumn::ReadPerSecond => dec_bytes_per_second_string(self.rps),
            ProcColumn::WritePerSecond => dec_bytes_per_second_string(self.wps),
            ProcColumn::TotalRead => dec_bytes_string(self.total_read),
            ProcColumn::TotalWrite => dec_bytes_string(self.total_write),
            ProcColumn::State => self.process_char.to_string(),
            ProcColumn::User => self
                .user
                .as_ref()
                .map(|user| user.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            ProcColumn::Time => format_time(self.time),
            #[cfg(feature = "gpu")]
            ProcColumn::GpuMemValue | ProcColumn::GpuMemPercent => self.gpu_mem_usage.to_string(),
            #[cfg(feature = "gpu")]
            ProcColumn::GpuUtilPercent => format!("{:.1}%", self.gpu_usage),
        }
    }
}

impl DataToCell<ProcColumn> for ProcWidgetData {
    fn to_cell_text(
        &self, column: &ProcColumn, calculated_width: NonZeroU16,
    ) -> Option<Cow<'static, str>> {
        // TODO: Optimize the string allocations here...
        // TODO: Also maybe just pull in the to_string call but add a variable
        // for the differences.
        Some(match column {
            #[cfg(unix)]
            ProcColumn::Nice => self.nice.to_string().into(),
            &ProcColumn::Priority => self.priority.to_string().into(),
            ProcColumn::CpuPercent => format!("{:.1}%", self.cpu_usage_percent).into(),
            ProcColumn::MemValue | ProcColumn::MemPercent => self.mem_usage.to_string().into(),
            ProcColumn::VirtualMem => binary_byte_string(self.virtual_mem).into(),
            ProcColumn::Swap => self
                .swap_bytes
                .map(binary_byte_string)
                .unwrap_or_else(|| "N/A".to_string())
                .into(),
            ProcColumn::Pid => self.pid.to_string().into(),
            ProcColumn::Count if self.is_group_member => "".into(),
            ProcColumn::Count => self.num_similar.to_string().into(),
            ProcColumn::Name | ProcColumn::Command => self.id.to_prefixed_string().into(),
            ProcColumn::ReadPerSecond => dec_bytes_per_second_string(self.rps).into(),
            ProcColumn::WritePerSecond => dec_bytes_per_second_string(self.wps).into(),
            ProcColumn::TotalRead => dec_bytes_string(self.total_read).into(),
            ProcColumn::TotalWrite => dec_bytes_string(self.total_write).into(),
            ProcColumn::State => {
                if calculated_width.get() < 8 {
                    self.process_char.to_string().into()
                } else {
                    self.process_state.into()
                }
            }
            ProcColumn::User => self
                .user
                .as_ref()
                .map(|user| user.to_string().into())
                .unwrap_or_else(|| "N/A".into()),
            ProcColumn::Time => format_time(self.time).into(),
            #[cfg(feature = "gpu")]
            ProcColumn::GpuMemValue | ProcColumn::GpuMemPercent => {
                self.gpu_mem_usage.to_string().into()
            }
            #[cfg(feature = "gpu")]
            ProcColumn::GpuUtilPercent => format!("{:.1}%", self.gpu_usage).into(),
        })
    }

    fn to_cell_line(
        &self, column: &ProcColumn, calculated_width: NonZeroU16, painter: &Painter,
    ) -> Option<Line<'static>> {
        match (column, &self.id.id_type) {
            (ProcColumn::Name | ProcColumn::Command, IdType::Split(split)) => Some(split_line(
                self.id.prefix.as_deref(),
                split,
                calculated_width.get().into(),
                painter.styles.highlighted_border_style,
                painter.styles.disabled_text_style,
            )),
            _ => None,
        }
    }

    #[cfg(target_os = "linux")]
    #[inline(always)]
    fn style_cell(&self, column: &ProcColumn, painter: &Painter) -> Option<ratatui::style::Style> {
        match column {
            ProcColumn::Name | ProcColumn::Command if self.process_type.is_thread() => {
                Some(painter.styles.thread_text_style)
            }
            _ => None,
        }
    }

    #[inline(always)]
    fn style_row<'a>(&self, row: Row<'a>, painter: &Painter) -> Row<'a> {
        if self.disabled {
            row.style(painter.styles.disabled_text_style)
        } else {
            row
        }
    }

    fn column_widths<C: DataTableColumn<ProcColumn>>(data: &[Self], columns: &[C]) -> Vec<u16>
    where
        Self: Sized,
    {
        let mut widths = vec![0; columns.len()];

        for d in data {
            for (w, c) in widths.iter_mut().zip(columns) {
                *w = max(*w, d.to_string(c.inner()).len() as u16);
            }
        }

        widths
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use ratatui::style::Color;

    use super::*;
    use crate::utils::data_units::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn test_split_widths_everything_fits() {
        let widths = split_widths(5, 10, 20, 100);
        assert_eq!(
            widths,
            SplitWidths {
                name: 5,
                cwd: Some(10),
                args: Some(20)
            }
        );
    }

    #[test]
    fn test_split_widths_options_keep_minimum_before_cwd() {
        // 41 = name 5 + separator 3 + 33 for the rest.
        let widths = split_widths(5, 50, 30, 41);
        assert_eq!(widths.cwd, Some(20));
        assert_eq!(widths.args, Some(10));

        // Short options keep only what they need; the directory gets more.
        let widths = split_widths(5, 50, 4, 41);
        assert_eq!(widths.cwd, Some(26));
        assert_eq!(widths.args, Some(4));

        // A short directory leaves the rest to the options.
        let widths = split_widths(5, 8, 30, 41);
        assert_eq!(widths.cwd, Some(8));
        assert_eq!(widths.args, Some(22));
    }

    #[test]
    fn test_split_widths_narrow() {
        // The name alone does not leave room for anything else.
        assert_eq!(
            split_widths(50, 10, 10, 20),
            SplitWidths {
                name: 20,
                cwd: None,
                args: None
            }
        );

        // No room for a second separator: the directory only.
        let widths = split_widths(5, 10, 10, 12);
        assert_eq!(widths.cwd, Some(4));
        assert_eq!(widths.args, None);

        // No options: the directory gets everything after the name.
        let widths = split_widths(5, 50, 0, 41);
        assert_eq!(widths.cwd, Some(33));
        assert_eq!(widths.args, None);
    }

    #[test]
    fn test_split_line() {
        let split = SplitCommand {
            name: "herdr".into(),
            cwd: Some("~/src/djbclark-ade/skills/bottom-config".into()),
            args: "server --handoff-import /tmp/sock".into(),
        };
        let separator_style = Style::default().fg(Color::Cyan);

        let line = split_line(Some("├─ "), &split, 50, separator_style, Style::default());
        let text = line_text(&line);
        assert_eq!(text, "├─ herdr ┊ ~/src/djbcl…/bottom-config ┊ server --…");
        assert_eq!(text.width(), 50);

        let separators = line
            .spans
            .iter()
            .filter(|span| span.content == SPLIT_SEPARATOR)
            .collect::<Vec<_>>();
        assert_eq!(separators.len(), 2);
        assert!(separators.iter().all(|span| span.style == separator_style));
    }

    #[test]
    fn test_split_line_unknown_cwd() {
        let split = SplitCommand {
            name: "WindowServer".into(),
            cwd: None,
            args: "-daemon".into(),
        };
        let unknown_style = Style::default().fg(Color::DarkGray);

        let line = split_line(None, &split, 80, Style::default(), unknown_style);
        assert_eq!(line_text(&line), "WindowServer ┊ ? ┊ -daemon");
        assert!(
            line.spans
                .iter()
                .any(|span| span.content == UNKNOWN_CWD && span.style == unknown_style)
        );
    }

    #[test]
    fn test_format_time() {
        const ONE_DAY: u64 = 24 * 60 * 60;

        assert_eq!(format_time(Duration::from_millis(500)), "0.500s");
        assert_eq!(format_time(Duration::from_millis(900)), "0.900s");
        assert_eq!(format_time(Duration::from_secs(1)), "1.000s");
        assert_eq!(format_time(Duration::from_secs(10)), "10.000s");
        assert_eq!(format_time(Duration::from_secs(60)), "1m 0.00s");
        assert_eq!(format_time(Duration::from_secs(61)), "1m 1.00s");
        assert_eq!(format_time(Duration::from_secs(600)), "10m 0.00s");
        assert_eq!(format_time(Duration::from_secs(601)), "10m 1.00s");
        assert_eq!(format_time(Duration::from_secs(3600)), "1h 0m 0s");
        assert_eq!(format_time(Duration::from_secs(3601)), "1h 0m 1s");
        assert_eq!(format_time(Duration::from_secs(3660)), "1h 1m 0s");
        assert_eq!(format_time(Duration::from_secs(3661)), "1h 1m 1s");
        assert_eq!(format_time(Duration::from_secs(ONE_DAY - 1)), "23h 59m 59s");
        assert_eq!(format_time(Duration::from_secs(ONE_DAY)), "1d 0h 0m");
        assert_eq!(format_time(Duration::from_secs(ONE_DAY + 1)), "1d 0h 0m");
        assert_eq!(format_time(Duration::from_secs(ONE_DAY + 60)), "1d 0h 1m");
        assert_eq!(
            format_time(Duration::from_secs(ONE_DAY + 3600 - 1)),
            "1d 0h 59m"
        );
        assert_eq!(format_time(Duration::from_secs(ONE_DAY + 3600)), "1d 1h 0m");
        assert_eq!(
            format_time(Duration::from_secs(ONE_DAY * 365 - 1)),
            "364d 23h 59m"
        );
    }

    #[test]
    fn test_binary_byte_string() {
        assert_eq!(binary_byte_string(0), "0B".to_string());
        assert_eq!(binary_byte_string(1), "1B".to_string());
        assert_eq!(binary_byte_string(1000), "1000B".to_string());
        assert_eq!(binary_byte_string(1023), "1023B".to_string());
        assert_eq!(binary_byte_string(KIBI_LIMIT), "1KiB".to_string());
        assert_eq!(binary_byte_string(KIBI_LIMIT + 1), "1KiB".to_string());
        assert_eq!(binary_byte_string(MEBI_LIMIT), "1MiB".to_string());
        assert_eq!(binary_byte_string(GIBI_LIMIT), "1.0GiB".to_string());
        assert_eq!(binary_byte_string(2 * GIBI_LIMIT), "2.0GiB".to_string());
        assert_eq!(
            binary_byte_string((2.5 * GIBI_LIMIT as f64) as u64),
            "2.5GiB".to_string()
        );
        assert_eq!(
            binary_byte_string((10.34 * TEBI_LIMIT as f64) as u64),
            "10.3TiB".to_string()
        );
        assert_eq!(
            binary_byte_string((10.36 * TEBI_LIMIT as f64) as u64),
            "10.4TiB".to_string()
        );
    }

    #[test]
    fn test_swap_addition() {
        let process = ProcessHarvest {
            swap_bytes: Some(100),
            ..Default::default()
        };
        let other = ProcessHarvest {
            swap_bytes: Some(200),
            ..Default::default()
        };
        let mut data = ProcWidgetData::from_data(&process, IdKind::Name, false);
        let other = ProcWidgetData::from_data(&other, IdKind::Name, false);

        data.add(&other);
        assert_eq!(data.swap_bytes, Some(300));

        let unknown = ProcWidgetData::from_data(&ProcessHarvest::default(), IdKind::Name, false);
        data.add(&unknown);
        assert_eq!(data.swap_bytes, Some(300));

        let known = ProcWidgetData::from_data(&process, IdKind::Name, false);
        let mut unknown =
            ProcWidgetData::from_data(&ProcessHarvest::default(), IdKind::Name, false);
        unknown.add(&known);
        assert_eq!(unknown.swap_bytes, Some(100));
    }
}
