# Fork note: feature request "expandable groups in grouped mode" (facts for djbclark to paraphrase)

Fork-only file. Same form as `upstream-feature-split-command.md` (feature_request.yml: two checkboxes and one
free-text box), in your own words. Best filed after, or together with a reference to, the split-command request: the
group rows are most useful when the members show their working directory and options.

## Searched first (2026-11-02, `gh search issues --repo ClementTsang/bottom`)

"expand group", "grouped tree": only **#1677 "Process tree does not work with group_processes flag"** (closed,
completed), which is about tree mode and grouping not combining, not about expanding a group. Mention it so the
maintainer sees it is a different request.

## What it is

Grouped mode (Tab) merges processes with the same name into one row with a Count column, and there is no way to see
which processes are inside. In btm-mt, with `split_command` on:

1. Processes are grouped by the split command name (so `python3.15 s1_search.py` and `python3.15 other.py` are
   separate groups).
2. A group of more than one process starts collapsed, shown as `+ name`; Space, `+` or `-` (or Right) expands it to
   `- name`, with each process listed under it as a tree branch (`├─` / `└─`) showing its own working directory and
   options; Left collapses it. The member rows have an empty Count cell and are sorted by the current sort column.
3. Killing the group row kills every process in it (as today); killing a member row kills only that process.

It reuses the tree-mode keys and drawing; the collapsed state is kept per group name.

## Why (your use case, in your words)

Grouped mode shows that "omp" has 28 processes using 50% CPU together, but not which one is busy or where each runs.
Today the only way is to leave grouped mode and hunt through the flat list.

## What it looks like (btm-mt, Tab, then the largest group expanded)

```
│Count▼ CPU%(c)  Command(n)                                                                            │
│28     50.7%    - omp                                                                                ▲│
│       0.0%     ├─ omp ┊ ~/src/paseo ┊ __omp_worker_lsp_mux                                          █│
│       2.2%     ├─ omp ┊ ~/src/djbclark-ade-deadline-registry-wt ┊ --model anthropic/claude-opus-5-5 acp │
│       0.1%     ├─ omp ┊ ~/src/djbclark-ade ┊ __omp_worker_lsp_mux                                    │
│       18.2%    ├─ omp ┊ ~/src/bottom ┊ --auto-approve                                                │
│       0.3%     ├─ omp ┊ ~/src/plektra-djbclark ┊ --mode rpc-ui --approval-mode yolo --thinking high … │
```

(Rows trimmed from a real capture on 2026-11-01.)

## Implementation, if asked

Working code in djbclark/bottom, branch `middle-truncate-command` (commit bf0e4663: `get_normal_data`,
`ProcWidgetData::group_head` / `group_member`, the collapse/expand functions in `src/widgets/process_table.rs`, and the
kill change in `src/app.rs`). It currently only applies with `split_command`; upstream might want it for the plain
name and command views too, where the members would differ only by PID and options.
