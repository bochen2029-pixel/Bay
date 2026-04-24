//! Lexicographic fractional-indexing helper.
//!
//! Port of the midpoint algorithm from David K. "Dan" Brown's
//! `fractional-indexing` (rocicorp/fractional-indexing, MIT). Alphabet is
//! 0–9 then a–z (36 digits, byte-sorted). Omits the integer-prefix
//! encoding from the reference because Bay's insert pattern never
//! saturates a single-character head digit in realistic solo usage; SPEC
//! §4.2 explicitly accepts growing rank strings as the cost of that
//! simplification.
//!
//! Invariants (caller must uphold or expect a panic):
//!   - When both bounds are `Some`, `a < b` lexicographically.
//!   - Existing ranks do not end in the smallest digit `'0'`. The
//!     algorithm never produces such a rank itself, so this holds by
//!     induction from an empty database.

const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Return a rank string strictly between `a` and `b` in lexicographic order.
///
/// `a == None` means "before the start"; `b == None` means "after the end".
pub fn rank_between(a: Option<&str>, b: Option<&str>) -> String {
    if let (Some(lo), Some(hi)) = (a, b) {
        assert!(
            lo < hi,
            "rank_between: expected a < b, got a={lo:?}, b={hi:?}"
        );
    }
    midpoint(a.unwrap_or(""), b)
}

fn digit_index(c: u8) -> usize {
    DIGITS
        .iter()
        .position(|&d| d == c)
        .unwrap_or_else(|| panic!("invalid rank digit: {:?}", c as char))
}

fn midpoint(a: &str, b: Option<&str>) -> String {
    // Strip longest common prefix. When `a` is shorter than `b`, treat the
    // missing positions in `a` as the smallest digit `'0'` — equivalent to
    // how the reference algorithm virtually right-pads `a` with zeros.
    if let Some(b_str) = b {
        let a_bytes = a.as_bytes();
        let b_bytes = b_str.as_bytes();
        let mut n = 0;
        while n < b_bytes.len() {
            let ac = *a_bytes.get(n).unwrap_or(&b'0');
            if ac != b_bytes[n] {
                break;
            }
            n += 1;
        }
        if n > 0 {
            let a_tail = if n >= a.len() { "" } else { &a[n..] };
            let b_tail = &b_str[n..];
            let suffix = midpoint(a_tail, Some(b_tail));
            return format!("{}{}", &b_str[..n], suffix);
        }
    }

    // No common prefix. Get first-digit indices; missing ⇒ boundary (0 for a,
    // end-of-alphabet for b).
    let da = a.as_bytes().first().map_or(0, |c| digit_index(*c));
    let db = match b {
        None => DIGITS.len(),
        Some(s) => s.as_bytes().first().map_or(DIGITS.len(), |c| digit_index(*c)),
    };

    if db > da + 1 {
        // Room for a single-digit midpoint strictly between da and db.
        let mid = (da + db) / 2;
        return (DIGITS[mid] as char).to_string();
    }

    // Digits are adjacent. If b has more characters beyond the first, the
    // single-char prefix b[..1] is itself strictly less than b and (because
    // of the prefix-stripping above and the no-trailing-zero invariant)
    // strictly greater than a.
    if let Some(b_str) = b {
        if b_str.len() > 1 {
            return b_str[..1].to_string();
        }
    }

    // Otherwise extend a by carrying its first digit and recursing with no
    // upper bound — any midpoint > "" works for the extension.
    let head = a.as_bytes().first().copied().unwrap_or(DIGITS[0]);
    let tail = if a.is_empty() { "" } else { &a[1..] };
    format!("{}{}", head as char, midpoint(tail, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each case asserts the *bounds property* (strictly between) rather
    // than an exact string — the algorithm is free to pick any valid
    // midpoint, and tests that pin the exact output would lock out
    // equivalent refactors.

    #[test]
    fn between_none_and_none() {
        let r = rank_between(None, None);
        assert!(!r.is_empty(), "rank must be non-empty, got {r:?}");
    }

    #[test]
    fn between_none_and_m() {
        let r = rank_between(None, Some("m"));
        assert!(r.as_str() < "m", "{r:?} must be < \"m\"");
        assert!(!r.is_empty());
    }

    #[test]
    fn between_a_and_none() {
        let r = rank_between(Some("a"), None);
        assert!(r.as_str() > "a", "{r:?} must be > \"a\"");
    }

    #[test]
    fn between_a_and_c() {
        let r = rank_between(Some("a"), Some("c"));
        assert!("a" < r.as_str() && r.as_str() < "c", "{r:?} must be in (\"a\", \"c\")");
    }

    #[test]
    fn between_a_and_b_extends() {
        let r = rank_between(Some("a"), Some("b"));
        assert!("a" < r.as_str() && r.as_str() < "b", "{r:?} must be in (\"a\", \"b\")");
        assert!(r.len() >= 2, "adjacent digits force an extension, got {r:?}");
    }

    #[test]
    fn repeated_inserts_between_two_keys_stay_strictly_ordered() {
        // Start with ("a", "c"), keep inserting between a and the previous
        // result. Every new rank must stay in (a, prev).
        let mut hi = "c".to_string();
        for _ in 0..20 {
            let r = rank_between(Some("a"), Some(&hi));
            assert!(r.as_str() > "a");
            assert!(r.as_str() < hi.as_str());
            hi = r;
        }
    }

    #[test]
    fn end_appends_stay_ordered() {
        // Simulate appending to an empty tier, then repeatedly adding
        // at the end.
        let mut prev: Option<String> = None;
        let mut last = "".to_string();
        for _ in 0..20 {
            let r = rank_between(prev.as_deref(), None);
            assert!(r.as_str() > last.as_str() || last.is_empty());
            last = r.clone();
            prev = Some(r);
        }
    }

    #[test]
    fn front_prepends_stay_ordered() {
        // Repeatedly insert before the current first rank.
        let mut first = "m".to_string();
        for _ in 0..20 {
            let r = rank_between(None, Some(&first));
            assert!(r.as_str() < first.as_str(), "{r:?} must be < {first:?}");
            first = r;
        }
    }

    #[test]
    #[should_panic(expected = "expected a < b")]
    fn panics_on_non_strict_bounds() {
        let _ = rank_between(Some("b"), Some("a"));
    }

    #[test]
    #[should_panic(expected = "expected a < b")]
    fn panics_on_equal_bounds() {
        let _ = rank_between(Some("m"), Some("m"));
    }
}
