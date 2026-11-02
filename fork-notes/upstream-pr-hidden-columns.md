# Fork note: upstream PR for #2297 (facts for djbclark to write the PR from)

Fork-only file. bottom's `AI_POLICY.md`: AI-written code is fine, but the PR description and any replies to
maintainers must be in your own words, the PR template must be filled in by a human, and AI context may only be
quoted (`>`) and disclosed. Upstream's template has a line for AI-generated changes: say so there.

## The branch

- Branch: `djbclark:fix/hidden-column-alignment`, one commit `ae1b6fcf` ("bug: skip hidden columns when pairing table
  columns with widths") on top of upstream `main` at `3d5ff667`. Only the fix; none of the fork's other changes.
- Open it at: https://github.com/ClementTsang/bottom/compare/main...djbclark:bottom:fix/hidden-column-alignment
- Files: `src/canvas/components/data_table/draw.rs` (+2), `src/canvas/components/data_table/sortable.rs` (+47/−4,
  most of it the test).
- Changelog: upstream lists every PR under `## 0.15.1 - Unreleased` in `CHANGELOG.md`, with the PR number, e.g.
  `- [#NNNN](https://github.com/ClementTsang/bottom/pull/NNNN): Fix ...`. Add it after opening the PR (the number is
  needed), as a second commit on the branch; it may go under a `### Bug Fixes` heading.

## What the change does

1. Drawing rows (`draw.rs`): the columns are filtered to the shown ones before being paired with the calculated
   widths.
2. Header (`sortable.rs`, `build_header`, both the plain and the sortable version): same filter; the sortable one keeps
   each column's real index so the sort arrow lands on the right column.
3. Header clicks (`sortable.rs`, `try_select_location`): `get_range` returns the position among shown columns; it is
   now mapped back to the real column index before `set_sort_index`.
4. Test `test_click_header_after_hidden_column`: three columns, the middle one hidden; a click on the second shown
   column must select column index 2, and a click on the first must select 0. Before the fix the first assertion
   gets index 1 (the hidden column).

## Testing evidence (run on djbclark's Mac, macOS 27, arm64)

Config `[processes] columns = ["pid", "cpu%", "state", "command"]`, then Tab (grouped mode):

```
== stock btm 0.15.0 (Homebrew)
│Count                   CPU%(c)▼                  State                                           │
│1                       5.7%                      Unknown                                        ▲│
│18                      4.8%                      Runnable                                       █│

== with the fix (btm-mt, same code)
│Count                   CPU%(c)▼                  Name(n)                                         │
│1                       6.1%                      WindowServer                                   ▲│
│3                       4.5%                      herdr                                          █│
```

CI: upstream's own `ci.yml`, dispatched on the fork for this branch: https://github.com/djbclark/bottom/actions/runs/41460037215.
23 jobs passed (Linux, Windows, FreeBSD and the cross targets, including format, clippy and test); the 5 macOS jobs were
cancelled because GitHub could not get a macOS runner ("The job was not acquired by Runner of type hosted even after
multiple attempts"), not because of the code. The same code built and passed fmt/clippy/test in the fork's own macOS
build (run 41379974927), and the PR's own CI run upstream will run the macOS jobs again.

Checklist items you can tick from this: `cargo fmt`, `cargo clippy --all -- -D warnings` and `cargo test` ran in that
CI; a new test was added; no dependency changes; tested on macOS (version above). Docs: none needed (bug fix).
