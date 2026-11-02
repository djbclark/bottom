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

/// The single-character ellipsis that marks cut text.
pub const ELLIPSIS: &str = "…";

/// The longest start of `content` that is at most `width` cells wide.
fn head_within(content: &str, width: usize) -> &str {
    let mut end = 0;
    let mut used = 0;
    for (idx, grapheme) in content.grapheme_indices(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used + grapheme_width > width {
            break;
        }
        used += grapheme_width;
        end = idx + grapheme.len();
    }
    &content[..end]
}

/// Truncates a path that is wider than `width` cells by replacing part of its
/// middle with [`ELLIPSIS`]. The last component is kept whole if it fits (it
/// takes precedence), and as much of the start as fits comes before it, e.g.
/// `~/src/djbclar…/scripts`. A path that fits is returned untouched.
///
/// If even the last component does not fit, that is all that is shown, cut at
/// its end. A path with one component is cut at its end.
pub fn truncate_path(path: &str, width: usize) -> Cow<'_, str> {
    if UnicodeWidthStr::width(path) <= width {
        return Cow::Borrowed(path);
    }

    let last_start = match path.trim_end_matches('/').rfind('/') {
        Some(index) if index > 0 => index,
        _ => return truncate_str(path, width),
    };
    // Starts with the `/` before the last component.
    let tail = &path[last_start..];
    let tail_width = UnicodeWidthStr::width(tail);

    if tail_width + ELLIPSIS.width() < width {
        let head = head_within(path, width - ELLIPSIS.width() - tail_width);
        Cow::Owned(concat_string!(head, ELLIPSIS, tail))
    } else if width == 0 {
        Cow::Borrowed("")
    } else {
        let last = truncate_str(&tail[1..], width - ELLIPSIS.width());
        Cow::Owned(concat_string!(ELLIPSIS, last))
    }
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

    use super::truncate_path;

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

    const PATH: &str = "~/src/djbclark-ade/skills/bottom-config/scripts";

    #[test]
    fn test_truncate_path_fits() {
        assert_eq!(truncate_path(PATH, PATH.len()), PATH);
    }

    #[test]
    fn test_truncate_path_keeps_last_component_and_start() {
        let result = truncate_path(PATH, 22);
        assert_eq!(result, "~/src/djbclar…/scripts");
        assert_eq!(UnicodeWidthStr::width(result.as_ref()), 22);

        // One cell more than the last component and the ellipsis keeps one
        // character of the start.
        assert_eq!(truncate_path(PATH, 10), "~…/scripts");
    }

    #[test]
    fn test_truncate_path_last_component_too_wide() {
        assert_eq!(truncate_path(PATH, 9), "…scripts");
        assert_eq!(truncate_path(PATH, 5), "…scr…");
        assert_eq!(truncate_path(PATH, 1), "…");
        assert_eq!(truncate_path(PATH, 0), "");
    }

    #[test]
    fn test_truncate_path_one_component_is_cut_at_end() {
        assert_eq!(truncate_path("/Applications", 6), "/Appl…");
    }

    #[test]
    fn test_truncate_path_wide_characters() {
        // Each character is 2 cells wide.
        let result = truncate_path("/日本語日本語/日本", 12);
        assert_eq!(result, "/日本…/日本");
        assert!(UnicodeWidthStr::width(result.as_ref()) <= 12);
    }
}
