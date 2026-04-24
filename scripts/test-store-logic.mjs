// Ad-hoc smoke test for the store's sort/insert helpers. No test framework
// dependency; run with `node scripts/test-store-logic.mjs`. Exits
// non-zero on failure. Not part of the app runtime.
//
// A real frontend test harness (vitest or similar) lands post-I-14 when
// the UI surface stabilizes.

function compareRank(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}

function insertByRank(ids, newId, itemsMap) {
  const newRank = itemsMap[newId].rank;
  const out = [];
  let inserted = false;
  for (const id of ids) {
    if (!inserted && compareRank(itemsMap[id].rank, newRank) > 0) {
      out.push(newId);
      inserted = true;
    }
    out.push(id);
  }
  if (!inserted) out.push(newId);
  return out;
}

function bucketByTier(items) {
  const byTier = { inbox: [], A: [], B: [], C: [] };
  const map = {};
  for (const item of items) {
    map[item.id] = item;
    byTier[item.tier].push(item.id);
  }
  for (const t of Object.keys(byTier)) {
    byTier[t].sort((a, b) => compareRank(map[a].rank, map[b].rank));
  }
  return { map, byTier };
}

let failures = 0;
function assertEq(actual, expected, label) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) {
    console.log(`  ok  ${label}`);
  } else {
    console.error(`  FAIL ${label}\n    actual:   ${a}\n    expected: ${e}`);
    failures++;
  }
}

// ── compareRank ──
{
  assertEq(compareRank("a", "b"), -1, "compareRank a < b");
  assertEq(compareRank("b", "a"), 1, "compareRank b > a");
  assertEq(compareRank("m", "m"), 0, "compareRank equal");
  assertEq(compareRank("ab", "b"), -1, "compareRank ab < b");
  assertEq(compareRank("a", "a0"), -1, "compareRank prefix shorter first");
}

// ── insertByRank: empty list ──
{
  const map = { x: { id: "x", rank: "m" } };
  assertEq(insertByRank([], "x", map), ["x"], "insert into empty");
}

// ── insertByRank: append at end (rank > all existing) ──
{
  const map = {
    a: { id: "a", rank: "a" },
    b: { id: "b", rank: "b" },
    c: { id: "c", rank: "c" },
  };
  assertEq(insertByRank(["a", "b"], "c", map), ["a", "b", "c"], "append at end");
}

// ── insertByRank: prepend at front (rank < all existing) ──
{
  const map = {
    a: { id: "a", rank: "m" },
    b: { id: "b", rank: "p" },
    front: { id: "front", rank: "b" },
  };
  assertEq(
    insertByRank(["a", "b"], "front", map),
    ["front", "a", "b"],
    "prepend at front",
  );
}

// ── insertByRank: middle ──
{
  const map = {
    a: { id: "a", rank: "a" },
    c: { id: "c", rank: "c" },
    mid: { id: "mid", rank: "b" },
  };
  assertEq(
    insertByRank(["a", "c"], "mid", map),
    ["a", "mid", "c"],
    "insert in middle",
  );
}

// ── bucketByTier: groups and sorts ──
{
  const items = [
    { id: "2", tier: "A", rank: "p" },
    { id: "1", tier: "A", rank: "m" },
    { id: "3", tier: "B", rank: "b" },
    { id: "4", tier: "inbox", rank: "a" },
  ];
  const { byTier } = bucketByTier(items);
  assertEq(byTier.A, ["1", "2"], "bucket A sorted by rank");
  assertEq(byTier.B, ["3"], "bucket B singleton");
  assertEq(byTier.C, [], "bucket C empty");
  assertEq(byTier.inbox, ["4"], "bucket inbox singleton");
}

// ── ordering invariant under repeated inserts ──
{
  const ranks = ["a", "c", "e", "g"];
  let ids = [];
  const map = {};
  for (const r of ranks) {
    const id = `id-${r}`;
    map[id] = { id, rank: r };
    ids = insertByRank(ids, id, map);
  }
  assertEq(ids.map((i) => map[i].rank), ranks, "inserts stay ordered");
}

if (failures > 0) {
  console.error(`\n${failures} failure(s)`);
  process.exit(1);
}
console.log("\nall store-logic checks passed");
