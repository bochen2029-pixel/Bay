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

    /// Cross-language parity oracle. Reads `scripts/rank-fixtures.json`
    /// (committed; regenerate via `cargo run --bin rank_fixture_gen`)
    /// and asserts every case's `expected` value matches what THIS
    /// implementation produces. The matching test on the TS side is
    /// `src/rank.parity.test.ts`. If the two implementations ever
    /// drift, exactly one of the two tests fails — the side whose
    /// output no longer matches the committed fixture.
    ///
    /// SPEC §4.2: rank algorithm is doctrine-locked. An intentional
    /// change requires regenerating the fixture (the JSON diff is
    /// reviewable in the same PR) and both verifiers re-pass at the
    /// new outputs.
    #[test]
    fn matches_committed_fixture() {
        let path: std::path::PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "..",
            "scripts",
            "rank-fixtures.json",
        ]
        .iter()
        .collect();
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let fixture: serde_json::Value = serde_json::from_str(&raw)
            .expect("rank-fixtures.json is valid JSON");

        let cases = fixture["cases"]
            .as_array()
            .expect("fixture.cases is an array");
        assert!(!cases.is_empty(), "fixture must contain at least one case");

        for (i, case) in cases.iter().enumerate() {
            let a = case["a"].as_str();
            let b = case["b"].as_str();
            let expected = case["expected"]
                .as_str()
                .unwrap_or_else(|| panic!("case {i}: expected is not a string"));
            let got = rank_between(a, b);
            assert_eq!(
                got, expected,
                "case {i} (a={a:?}, b={b:?}): expected {expected:?}, got {got:?}",
            );
        }
    }

    // ── Property tests (non-LLM oracle for rank_between) ───────────
    //
    // These are the Externality Principle's mechanical check on
    // rank_between: structural laws that must hold for ALL valid
    // inputs, independent of anyone's interpretation of the expected
    // output. If a future refactor breaks the bounds property, the
    // existing unit tests (which assert specific bounds on a handful
    // of cases) might miss it; the property test catches it.
    //
    // Written as plain #[test] fns that call proptest's TestRunner
    // directly — the `proptest!` macro's fn-form has edge cases around
    // zero-arg tests and meta attributes; the explicit TestRunner form
    // is more verbose but unambiguous and compiles reliably across
    // proptest versions.

    use proptest::prelude::*;

    /// Generate a valid rank string: a non-empty sequence of base-36
    /// digits, not ending in '0' (the no-trailing-zero invariant the
    /// algorithm preserves inductively).
    fn valid_rank_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec("[0-9a-z]", 1..=8)
            .prop_map(|chars| chars.join(""))
            .prop_filter("no trailing '0'", |s| !s.ends_with('0'))
    }

    /// Property 1: rank_between(a, b) is strictly between a and b
    /// for all valid (a, b) with a < b.
    #[test]
    fn prop_strictly_between_both_bounded() {
        proptest!(|(a in valid_rank_strategy(), b in valid_rank_strategy())| {
            // Skip cases where a >= b (rank_between would panic by design).
            prop_assume!(a < b);
            let mid = rank_between(Some(&a), Some(&b));
            prop_assert!(mid.as_str() > a.as_str(), "mid {mid:?} must be > a {a:?}");
            prop_assert!(mid.as_str() < b.as_str(), "mid {mid:?} must be < b {b:?}");
        });
    }

    /// Property 2: rank_between(None, b) < b for all valid b.
    #[test]
    fn prop_strictly_below_upper_bound() {
        proptest!(|(b in valid_rank_strategy())| {
            let mid = rank_between(None, Some(&b));
            prop_assert!(!mid.is_empty());
            prop_assert!(mid.as_str() < b.as_str());
        });
    }

    /// Property 3: rank_between(a, None) > a for all valid a.
    #[test]
    fn prop_strictly_above_lower_bound() {
        proptest!(|(a in valid_rank_strategy())| {
            let mid = rank_between(Some(&a), None);
            prop_assert!(!mid.is_empty());
            prop_assert!(mid.as_str() > a.as_str());
        });
    }

    /// Property 4: rank_between(None, None) is non-empty (a valid
    /// standalone rank) and preserves the no-trailing-zero invariant.
    #[test]
    fn prop_unbounded_returns_nonempty() {
        let mid = rank_between(None, None);
        assert!(!mid.is_empty(), "mid must be non-empty, got {mid:?}");
        assert!(!mid.ends_with('0'), "mid must not end in '0', got {mid:?}");
    }

    /// Property 5: the no-trailing-zero invariant. The algorithm
    /// must never produce a rank ending in '0' (it preserves this
    /// inductively from an empty database). This is the precondition
    /// that makes future rank_between calls valid.
    #[test]
    fn prop_never_trailing_zero() {
        proptest!(|(a in prop::option::of(valid_rank_strategy()),
                    b in prop::option::of(valid_rank_strategy()))| {
            // Skip invalid (Some, Some) pairs where a >= b.
            if let (Some(ref a_str), Some(ref b_str)) = (&a, &b) {
                prop_assume!(a_str < b_str);
            }
            let mid = rank_between(a.as_deref(), b.as_deref());
            prop_assert!(!mid.ends_with('0'),
                "mid {mid:?} must not end in '0' (no-trailing-zero invariant)");
        });
    }

    /// Property 6: monotonicity under repeated front-insertion.
    #[test]
    fn prop_front_inserts_are_monotone_decreasing() {
        proptest!(|(n in 1u32..30)| {
            let mut first = "m".to_string();
            for _ in 0..n {
                let r = rank_between(None, Some(&first));
                prop_assert!(r.as_str() < first.as_str(), "front insert must decrease");
                first = r;
            }
        });
    }

    /// Property 7: monotonicity under repeated end-insertion.
    #[test]
    fn prop_end_inserts_are_monotone_increasing() {
        proptest!(|(n in 1u32..30)| {
            let mut prev: Option<String> = None;
            let mut last = "".to_string();
            for _ in 0..n {
                let r = rank_between(prev.as_deref(), None);
                prop_assert!(r.as_str() > last.as_str() || last.is_empty(),
                    "end insert must increase");
                last = r.clone();
                prev = Some(r);
            }
        });
    }

    /// Property 8: midpoint idempotent under repeated insertion
    /// between two fixed bounds — every new rank stays strictly
    /// between the bounds, and the sequence is monotone.
    #[test]
    fn prop_midpoint_inserts_stay_between_bounds() {
        proptest!(|(n in 1u32..30)| {
            let lo = "a".to_string();
            let mut cur_hi = "c".to_string();
            for _ in 0..n {
                let r = rank_between(Some(&lo), Some(&cur_hi));
                prop_assert!(r.as_str() > lo.as_str(), "must stay > lo");
                prop_assert!(r.as_str() < cur_hi.as_str(), "must stay < cur_hi");
                cur_hi = r;
            }
        });
    }
}
