//! Regenerates `scripts/rank-fixtures.json` from Rust's `rank_between`
//! implementation. Run on demand:
//!
//!     cargo run --bin rank_fixture_gen
//!
//! The fixture is the cross-language parity oracle — both the Rust
//! test (`domain::rank::tests::matches_committed_fixture`) and the
//! Vitest test (`src/rank.parity.test.ts`) read it and assert their
//! own implementation produces the same output for every case.
//! Diverging output on either side fails the relevant test loudly.
//!
//! When the rank algorithm changes intentionally (which is rare —
//! the algorithm is doctrine-locked per SPEC §4.2), regenerate the
//! fixture and review the JSON diff in code review. Both verifiers
//! will then re-pass at the new outputs.
//!
//! The input set below is hand-curated to cover the algorithm's
//! interesting branches: prefix stripping, single-digit midpoint,
//! adjacent-digits extension, asymmetric-length pairs, and the
//! frontier of the `[None, ?]` / `[?, None]` boundaries. It is
//! deliberately NOT generated from chained calls of `rank_between`
//! itself — chained inputs depend on the algorithm's pick at the
//! previous step, which means a regression that picks a different
//! valid midpoint would silently shift the whole chain and the test
//! would still pass.

use bay_lib::domain::rank_between;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Inputs that must produce a stable, byte-identical output across
    // Rust and the TS port. (a, b) lexicographic order is required.
    let inputs: &[(Option<&str>, Option<&str>)] = &[
        // ── boundaries ───────────────────────────────────────────
        (None, None),
        (None, Some("0")),
        (None, Some("a")),
        (None, Some("m")),
        (None, Some("z")),
        (None, Some("0a")),
        (None, Some("0z")),
        (Some("0"), None),
        (Some("a"), None),
        (Some("m"), None),
        (Some("y"), None),
        (Some("z"), None),
        (Some("zz"), None),

        // ── wide gaps (single-char midpoint) ────────────────────
        (Some("a"), Some("c")),
        (Some("a"), Some("z")),
        (Some("0"), Some("z")),
        (Some("0"), Some("9")),
        (Some("c"), Some("g")),
        (Some("d"), Some("o")),
        (Some("0"), Some("a")),
        (Some("9"), Some("a")),

        // ── adjacent digits (extension required) ────────────────
        (Some("0"), Some("1")),
        (Some("8"), Some("9")),
        (Some("a"), Some("b")),
        (Some("b"), Some("c")),
        (Some("y"), Some("z")),

        // ── multi-char same first digit ─────────────────────────
        (Some("aa"), Some("ab")),
        (Some("ab"), Some("ac")),
        (Some("a0"), Some("a1")),
        (Some("a8"), Some("a9")),
        (Some("ay"), Some("az")),
        (Some("az"), Some("b")),

        // ── multi-char different first digit ────────────────────
        (Some("a0"), Some("b0")),
        (Some("ab"), Some("z")),
        (Some("ay"), Some("c")),

        // ── asymmetric lengths ─────────────────────────────────
        // Empty prefix-padding interpretation: shorter string is
        // virtually right-padded with '0' digits.
        (Some("a"), Some("aa")),
        (Some("a"), Some("ab")),
        (Some("a"), Some("az")),
        (Some("aa"), Some("b")),
        (Some("ab"), Some("b")),

        // ── deeply-nested adjacent ─────────────────────────────
        (Some("aab"), Some("ab")),
        (Some("a"), Some("aab")),
    ];

    let cases: Vec<serde_json::Value> = inputs
        .iter()
        .map(|(a, b)| {
            let expected = rank_between(*a, *b);
            json!({
                "a": a,
                "b": b,
                "expected": expected,
            })
        })
        .collect();

    let out = json!({
        "version": 1,
        "generator": "src-tauri/src/bin/rank_fixture_gen.rs",
        "note": concat!(
            "Regenerate via `cargo run --bin rank_fixture_gen`. ",
            "Both Rust (domain::rank::tests::matches_committed_fixture) ",
            "and TS (src/rank.parity.test.ts) verify their own output ",
            "against this fixture; diverging output on either side ",
            "fails the relevant test."
        ),
        "case_count": cases.len(),
        "cases": cases,
    });

    // Resolve the output path relative to this crate's manifest dir
    // so the bin works regardless of where it's invoked from.
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "scripts",
        "rank-fixtures.json",
    ]
    .iter()
    .collect();

    let text = serde_json::to_string_pretty(&out).expect("serialize fixture");
    fs::write(&path, text + "\n").expect("write fixture");

    println!(
        "Wrote {} cases to {}",
        inputs.len(),
        path.display()
    );
}
