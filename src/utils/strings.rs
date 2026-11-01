use std::borrow::Cow;

use concat_string::concat_string;
use ratatui::text::Text;
use unicode_ellipsis::truncate_str;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Truncates text if it is too long, and adds an ellipsis at the end if needed.
///
/// TODO: Maybe cache results from this function for some cases? e.g. columns
#[inline]
pub fn truncate_to_text<'a, U: Into<usize>>(content: &str, width: U) -> Text<'a> {
    Text::raw(truncate_str(content, width.into()).to_string())
}

/// Truncates text that is wider than `width` cells by replacing its middle with
/// `[...]`, so that both the start and the end are kept. Text that already fits
/// is returned untouched.
///
/// If `width` is too small to keep anything on both sides of the marker, this
/// falls back to [`truncate_str`] (cut at the end, with an ellipsis).
pub fn truncate_middle(content: &str, width: usize) -> Cow<'_, str> {
    const MARKER: &str = "[...]";
    const MARKER_WIDTH: usize = MARKER.len();

    if UnicodeWidthStr::width(content) <= width {
        return Cow::Borrowed(content);
    }

    if width < MARKER_WIDTH + 2 {
        return truncate_str(content, width);
    }

    // The head and tail budgets add up to less than the content's width, so
    // the kept head and tail can never overlap.
    let budget = width - MARKER_WIDTH;
    let head_budget = budget.div_ceil(2);
    let tail_budget = budget / 2;

    let mut head_end = 0;
    let mut used = 0;
    for (idx, grapheme) in content.grapheme_indices(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used + grapheme_width > head_budget {
            break;
        }
        used += grapheme_width;
        head_end = idx + grapheme.len();
    }

    let mut tail_start = content.len();
    used = 0;
    for (idx, grapheme) in content.grapheme_indices(true).rev() {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used + grapheme_width > tail_budget {
            break;
        }
        used += grapheme_width;
        tail_start = idx;
    }

    Cow::Owned(concat_string!(
        &content[..head_end],
        MARKER,
        &content[tail_start..]
    ))
}

/// Checks that the first string is equal to any of the other ones in a ASCII
/// case-insensitive match.
///
/// The generated code is the same as writing:
/// `to_ascii_lowercase(a) == to_ascii_lowercase(b) || to_ascii_lowercase(a) ==
/// to_ascii_lowercase(c)`, but without allocating and copying temporaries.
///
/// # Examples
///
/// ```ignore
/// assert!(multi_eq_ignore_ascii_case!("test", "test"));
/// assert!(multi_eq_ignore_ascii_case!("test", "a" | "b" | "test"));
/// assert!(!multi_eq_ignore_ascii_case!("test", "a" | "b" | "c"));
/// ```
#[macro_export]
macro_rules! multi_eq_ignore_ascii_case {
    ( $lhs:expr, $last:literal ) => {
        $lhs.eq_ignore_ascii_case($last)
    };
    ( $lhs:expr, $head:literal | $($tail:tt)* ) => {
        $lhs.eq_ignore_ascii_case($head) || multi_eq_ignore_ascii_case!($lhs, $($tail)*)
    };
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::truncate_middle;

    #[test]
    fn test_multi_eq_ignore_ascii_case() {
        assert!(
            multi_eq_ignore_ascii_case!("test", "test"),
            "single comparison should succeed"
        );
        assert!(
            multi_eq_ignore_ascii_case!("test", "a" | "test"),
            "double comparison should succeed"
        );
        assert!(
            multi_eq_ignore_ascii_case!("test", "a" | "b" | "test"),
            "multi comparison should succeed"
        );

        assert!(
            !multi_eq_ignore_ascii_case!("test", "a"),
            "single non-matching should fail"
        );
        assert!(
            !multi_eq_ignore_ascii_case!("test", "a" | "b"),
            "double non-matching should fail"
        );
        assert!(
            !multi_eq_ignore_ascii_case!("test", "a" | "b" | "c"),
            "multi non-matching should fail"
        );
    }

    #[test]
    fn test_truncate_middle_fits() {
        assert_eq!(truncate_middle("abc", 5), "abc");
        assert_eq!(truncate_middle("abcde", 5), "abcde");
    }

    #[test]
    fn test_truncate_middle_keeps_both_ends() {
        let cmd = "/usr/local/bin/some-very-long-command --flag";
        let result = truncate_middle(cmd, 20);
        assert_eq!(result, "/usr/loc[...] --flag");
        assert_eq!(UnicodeWidthStr::width(result.as_ref()), 20);
    }

    #[test]
    fn test_truncate_middle_narrow_falls_back_to_end() {
        for width in 0..7 {
            let result = truncate_middle("abcdefghij", width);
            assert!(
                UnicodeWidthStr::width(result.as_ref()) <= width,
                "width {width} gave {result:?}"
            );
            assert!(!result.contains("[...]"), "width {width} gave {result:?}");
        }
    }

    #[test]
    fn test_truncate_middle_wide_characters() {
        // Each character is 2 cells wide; a 5-cell budget side keeps 2 of them.
        let result = truncate_middle("日本語日本語日本語日本語", 15);
        assert_eq!(result, "日本[...]本語");
        assert!(UnicodeWidthStr::width(result.as_ref()) <= 15);
    }
}
