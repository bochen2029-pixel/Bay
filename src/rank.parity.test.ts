// Cross-language parity test for rank_between (TS side).
//
// Reads `scripts/rank-fixtures.json` — the committed fixture is
// generated from the Rust implementation
// (`cargo run --bin rank_fixture_gen`) — and asserts every case's
// `expected` value matches what the TS `rankBetween` produces.
//
// The matching test on the Rust side is
// `domain::rank::tests::matches_committed_fixture`. If the two
// implementations ever drift, exactly one of the two tests fails —
// the side whose output no longer matches the committed fixture.
//
// Why this test exists: pre-v1.1, the only check that the TS
// `rankBetween` matched the Rust `rank_between` was code review.
// The two implementations are short but each has subtle branches
// (prefix stripping, asymmetric-length pad-with-zero,
// adjacent-digits extension) where divergence would silently corrupt
// item ordering — items created on one side could land in the wrong
// slot relative to items created on the other side.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { rankBetween } from "./rank";

type RankFixtureCase = {
  a: string | null;
  b: string | null;
  expected: string;
};

type RankFixture = {
  version: number;
  case_count: number;
  cases: RankFixtureCase[];
};

function loadFixture(): RankFixture {
  // Vitest's cwd is the repo root (where vite.config.ts lives), so
  // the `scripts/` path resolves correctly.
  const path = resolve(process.cwd(), "scripts/rank-fixtures.json");
  const raw = readFileSync(path, "utf8");
  return JSON.parse(raw) as RankFixture;
}

describe("rankBetween parity with Rust fixture", () => {
  const fixture = loadFixture();

  it("the fixture has at least one case", () => {
    expect(fixture.cases.length).toBeGreaterThan(0);
    expect(fixture.cases.length).toBe(fixture.case_count);
  });

  it.each(fixture.cases)(
    "rankBetween($a, $b) === $expected",
    ({ a, b, expected }) => {
      const got = rankBetween(a, b);
      expect(got).toBe(expected);
    },
  );
});
