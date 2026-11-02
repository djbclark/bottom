# Fork note: upstream issue for the hidden-column bug

Fork-only file (djbclark/bottom, branch `middle-truncate-command`); keep it out of any upstream PR.

**Filed by djbclark as https://github.com/ClementTsang/bottom/issues/2297** (2026-11-02). The fix PR is prepared:
see `upstream-pr-hidden-columns.md`.

The fork's fix is commit 846e4874 ("tables skip hidden columns when pairing columns with widths"), with a regression
test in `src/canvas/components/data_table/sortable.rs` (`test_click_header_after_hidden_column`).

## The draft it was paraphrased from

**Grouped mode shows the wrong columns when a hidden column isn't last**

With `[processes] columns = ["pid", "cpu%", "state", "command"]`, press Tab. Grouped mode hides State, but the table
still shows a State column and drops Command. The default columns are affected the same way: User and State are
hidden but sit before Time and Priority.

The cause is that `calculate_column_widths` skips hidden columns, while the row drawing (`draw.rs`), `build_header`
and the header-click mapping (`get_range` → `set_sort_index`) pair *every* column with those widths. Everything after
a hidden column is shifted by one. Filtering out hidden columns before pairing fixes it; the click index then has to
be mapped back to the real column index.

Seen in 0.15.0 on macOS.
