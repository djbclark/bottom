# Processes Configuration

## Settings

If you want to change some of the default behaviour of the processes widget, you can configure some things in the config file.

| Field                   | Type    | Functionality                                                                                             |
| ----------------------- | ------- | --------------------------------------------------------------------------------------------------------- |
| `get_threads`           | Boolean | Gather process thread information.                                                                        |
| `hide_k_threads`        | Boolean | Hide kernel threads from being shown. Linux only.                                                         |
| `tree_collapse`         | Boolean | Collapse the process tree by default when tree mode is set.                                               |
| `process_command`       | Boolean | Shows the full command name instead of the process name by default.                                       |
| `split_command`         | Boolean | When a full command is shown, splits it into name, working directory and options. See below.              |
| `disable_advanced_kill` | Boolean | Disable the advanced kill dialog and just show the basic one with no options. Linux, macOS, FreeBSD only. |
| `default_memory_value`  | Boolean | Defaults to showing process memory usage by value.                                                        |
| `default_grouped`       | Boolean | Groups processes with the same name by default. No effect if `--tree` is set.                             |
| `regex`                 | Boolean | Enables regex by default while searching.                                                                 |
| `case_sensitive`        | Boolean | Enables case sensitivity by default when searching.                                                       |
| `whole_word`            | Boolean | Enables whole-word matching by default while searching.                                                   |
| `default_tree`          | Boolean | Makes the process widget use tree mode by default.                                                        |
| `current_usage`         | Boolean | Calculates process CPU usage as a percentage of current usage rather than total usage.                    |
| `unnormalized_cpu`      | Boolean | Show process CPU% usage without averaging over the number of CPU cores.                                   |

## Columns

You can configure which columns are shown by the process widget by setting the `columns` setting:

```toml
[processes]
# Pick which columns you want to use in any order.
columns = ["cpu%", "mem%", "pid", "name", "read", "write", "tread", "twrite", "state", "user", "time", "gmem%", "gpu%"]
```

## Default Sort Column

By default, the process widget starts sorted by CPU usage in descending order. You can change the column it sorts by at startup:

```toml
[processes]
default_sort = "mem"
```

Any of the column names accepted by `columns` work here (e.g. `"cpu%"`, `"mem"`, `"pid"`, `"name"`, `"read"`). If the
column you pick is not actually shown by the widget, the built-in default is used instead.

The same setting is also exposed as a CLI flag, which takes precedence over the config file:

```
btm --process_default_sort mem
```

You can also set the default order by changing `processes.sort_order` with `"Ascending"` or `"Descending"`. If not set, then by default, it will use a sort order that is reasonable (e.g. descending for CPU/memory usage, ascending for process names).

```toml
[processes]
sort_order = "Ascending"
```

## Split Command

With `split_command = true`, a full command (`process_command`, or `P`) is shown in three parts, separated by `┊` in the
selected border colour: the command name, the working directory, and the options.

```toml
[processes]
process_command = true
split_command = true
```

- The command name is the executable's file name. For a script interpreter (`python`, `node`, `bun`, `deno`, `ruby`,
  `perl`, `php`, shells, ...) it is the interpreter followed by the script's file name, e.g. `python3.15 s1_search.py`
  (or the module, for `python -m`).
- The working directory shows your home directory as `~`. A process whose directory cannot be read (usually one of
  another user, unless bottom runs as root) shows a dim `?`.
- The options are the remaining arguments; ones with spaces or quotes are single-quoted.

When the column is too narrow, the name is kept whole; the options keep at least 10 cells; the working directory is cut
in the middle with `…`, keeping its last directory whole and as much of its start as fits; the options are cut at the
end with `…`.

In grouped mode (`Tab`), processes are grouped by this command name. A group of more than one process starts collapsed
(`+`); expand it (`-`) with `Right`, `Space` or `-` to list each process with its own working directory and options,
and collapse it with `Left`, `Space` or `-`. Killing a group's row kills all its processes; killing a row under it
kills just that process.
