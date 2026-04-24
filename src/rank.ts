// Lexicographic fractional-indexing helper — TS port of
// src-tauri/src/domain/rank.rs. Both implementations follow Dan Brown's
// fractional-indexing (rocicorp/fractional-indexing, MIT) midpoint
// algorithm. Alphabet is 0–9 then a–z (36 digits, byte-sorted).
//
// The two implementations must produce ranks that are mutually
// compatible (strictly-between their inputs, lex-ordered). They need
// not produce identical outputs for the same inputs — the frontend
// produces ranks during drag, the backend produces ranks during
// create. Both live in the same ordering.
//
// Invariant (caller-upheld, or expect a thrown error):
//   - when both bounds are non-null, `a < b` lexicographically
//   - existing ranks never end in the smallest digit '0'; this holds
//     by induction from the empty store

const DIGITS = "0123456789abcdefghijklmnopqrstuvwxyz";

export function rankBetween(a: string | null, b: string | null): string {
  if (a !== null && b !== null && !(a < b)) {
    throw new Error(
      `rankBetween: expected a < b, got a=${JSON.stringify(a)} b=${JSON.stringify(b)}`,
    );
  }
  return midpoint(a ?? "", b);
}

function digitIndex(c: string): number {
  const i = DIGITS.indexOf(c);
  if (i < 0) {
    throw new Error(`invalid rank digit: ${JSON.stringify(c)}`);
  }
  return i;
}

function midpoint(a: string, b: string | null): string {
  // Strip longest common prefix; virtually right-pad the shorter `a`
  // with the smallest digit so (a="ab", b="abcd") leaves ("", "cd").
  if (b !== null) {
    let n = 0;
    while (n < b.length) {
      const ac = n < a.length ? a[n] : "0";
      if (ac !== b[n]) break;
      n++;
    }
    if (n > 0) {
      const aTail = n >= a.length ? "" : a.substring(n);
      const bTail = b.substring(n);
      const suffix = midpoint(aTail, bTail);
      return b.substring(0, n) + suffix;
    }
  }

  // No common prefix.
  const da = a.length === 0 ? 0 : digitIndex(a[0]);
  const db = b === null ? DIGITS.length : digitIndex(b[0]);

  if (db > da + 1) {
    // Room for a single-digit midpoint strictly between da and db.
    const mid = Math.floor((da + db) / 2);
    return DIGITS[mid];
  }

  // Adjacent digits. If b has more chars beyond the first, the
  // one-char prefix is itself strictly less than b and (by the
  // no-trailing-zero invariant) strictly greater than a.
  if (b !== null && b.length > 1) {
    return b.substring(0, 1);
  }

  // Extend a by carrying its first digit and recursing with no upper
  // bound.
  const head = a.length === 0 ? DIGITS[0] : a[0];
  const tail = a.length === 0 ? "" : a.substring(1);
  return head + midpoint(tail, null);
}
