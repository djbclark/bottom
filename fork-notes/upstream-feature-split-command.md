# Fork note: feature request "split command" with working directory (facts for djbclark to paraphrase)

Fork-only file. File with upstream's feature request form
(https://github.com/ClementTsang/bottom/issues/new?template=feature_request.yml): two checkboxes (searched docs and
open issues; read the AI policy) and one free-text "Describe the feature request" box (what, why, use cases, and
implementation ideas if any). Write it in your own words; this sheet is only the facts.

Open this after the #2297 PR has had a response, so you know how the maintainer reacts.

## Searched first (2026-11-02, `gh search issues --repo ClementTsang/bottom`)

"working directory", "cwd", "command arguments column", "truncate command", "command name column split": the only
related open issue is **#363 "Replicate htop's process list/tree options"**, which lists htop's "Show program path" and
"Highlight program basename" ("really helps with a mile-long commands"). This request is close to those two; link it.

## What it is

With `[processes] split_command = true` (and the full command shown, `process_command` / `P`), the Command cell shows
three parts: the command name, the working directory, and the options, separated by `┊` in the theme's
`selected_border_colour`.

1. Command name: the executable's file name. For a script interpreter (python, node, bun, deno, ruby, perl, php,
   shells, lua, osascript...) it is the interpreter plus the script's file name, e.g. `python3.15 s1_search.py`
   (or the module for `python -m`). Without that, every Python script is just "python".
2. Working directory: home shown as `~`; a directory that cannot be read (other users' processes unless bottom runs as
   root) shows a dim `?`. Collected only when the option is on (one extra per-process read: `/proc/<pid>/cwd` on
   Linux, sysinfo's cwd elsewhere).
3. Options: the remaining arguments, ones with spaces or quotes single-quoted.

When narrow: the name is kept whole; options keep at least 10 cells; the directory gets what it needs of the rest and
is cut in the middle with `…`, keeping its last component whole and as much of the start as fits
(`~/src/djbclar…/scripts`); options are cut at the end with `…`.

A search keyword `cwd=` matches the working directory as shown (`cwd=~/src/bottom`), like `user=` / `state=`.

## Why (your use case, in your words)

Finding what drives load on a machine with many similar processes: several `omp`, `claude`, `node` or `python`
processes are indistinguishable by name, and the full command line is too long to read. Which project directory each
runs in is usually the deciding fact, and stock bottom cannot show it at all.

## What it looks like (btm-mt, 110 columns)

```
│PID(p)  CPU%(c)▼ State     Command(n)                                                                       │
│9915    45.2%    Runnable  Python relay.py ┊ ~/src/djbclark-ade ┊ -I --why d2a505dd-8c93-4ba5-8684-e98a386…▲│
│430     40.0%    Unknown   WindowServer ┊ ?                                                                █│
│79092   24.4%    Runnable  Python s1_search.py ┊ / ┊ -I index                                               │
│33427   16.8%    Runnable  herdr ┊ / ┊ server --handoff-import /Users/djbclark/.config/herdr/herdr-handoff… │
```

`cwd=~/src/bottom` in search:

```
│10344   4.3%     Runnable  btm-mt ┊ ~/src/bottom ┊ -C /Users/djbclark/.config/bottom/bottom.toml -e --defa…█│
```

## Implementation, if asked

Working code exists in djbclark/bottom, branch `middle-truncate-command` (commits bf0e4663, a97498fc for the split;
86c31fa8 for `cwd=`; the docs are in `docs/content/configuration/config-file/processes.md` "Split Command" and the
search keyword table). A PR would be rebased onto upstream `main` and split from the grouping change. Open questions a
maintainer may have: the config key name, whether the separator should be configurable, and whether the interpreter
list belongs in core.
