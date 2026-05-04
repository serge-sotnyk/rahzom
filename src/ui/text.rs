//! Text helpers for TUI rendering.

use std::borrow::Cow;

/// Middle-ellipsis truncation by Unicode chars.
///
/// If the input fits within `max` chars, returns it borrowed. Otherwise returns
/// `"<head>…<tail>"` whose total char count equals `max`. Splits the budget so
/// the head gets `ceil((max - 1) / 2)` and the tail the rest. For `max < 2`
/// returns `"…"` (or `""` if `max == 0`).
pub fn truncate_middle(s: &str, max: usize) -> Cow<'_, str> {
    let len = s.chars().count();
    if len <= max {
        return Cow::Borrowed(s);
    }
    if max == 0 {
        return Cow::Owned(String::new());
    }
    if max == 1 {
        return Cow::Owned("…".to_string());
    }

    let budget = max - 1;
    let head_len = budget.div_ceil(2);
    let tail_len = budget - head_len;

    let head: String = s.chars().take(head_len).collect();
    let tail: String = s.chars().skip(len - tail_len).collect();

    let mut out = String::with_capacity(head.len() + tail.len() + "…".len());
    out.push_str(&head);
    out.push('…');
    out.push_str(&tail);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_chars(s: &str) -> usize {
        s.chars().count()
    }

    #[test]
    fn empty_input() {
        assert_eq!(truncate_middle("", 5), "");
        assert_eq!(truncate_middle("", 0), "");
    }

    #[test]
    fn fits_as_is_returns_borrowed() {
        let res = truncate_middle("abc", 10);
        assert!(matches!(res, Cow::Borrowed(_)));
        assert_eq!(res, "abc");
    }

    #[test]
    fn boundary_equal_length() {
        let res = truncate_middle("abcde", 5);
        assert_eq!(res, "abcde");
    }

    #[test]
    fn ascii_long_truncated() {
        let res = truncate_middle("abcdefghijklmno", 7);
        assert_eq!(count_chars(&res), 7);
        assert!(res.starts_with("abc"));
        assert!(res.ends_with("mno"));
        assert!(res.contains('…'));
    }

    #[test]
    fn ascii_long_even_budget() {
        let res = truncate_middle("abcdefghij", 5);
        assert_eq!(count_chars(&res), 5);
        // budget = 4 => head = 2 (ceil), tail = 2
        assert_eq!(res, "ab…ij");
    }

    #[test]
    fn ascii_long_odd_budget() {
        let res = truncate_middle("abcdefghij", 6);
        assert_eq!(count_chars(&res), 6);
        // budget = 5 => head = 3, tail = 2
        assert_eq!(res, "abc…ij");
    }

    #[test]
    fn max_zero() {
        assert_eq!(truncate_middle("abcdef", 0), "");
    }

    #[test]
    fn max_one() {
        assert_eq!(truncate_middle("abcdef", 1), "…");
    }

    #[test]
    fn max_two() {
        let res = truncate_middle("abcdef", 2);
        // budget = 1 => head = 1, tail = 0
        assert_eq!(res, "a…");
        assert_eq!(count_chars(&res), 2);
    }

    #[test]
    fn unicode_mix() {
        // Cyrillic input. We truncate by chars, not bytes, so the result
        // length must be measured in chars.
        let res = truncate_middle("привет_всем_добрый_день", 7);
        assert_eq!(count_chars(&res), 7);
        assert!(res.contains('…'));
    }
}
