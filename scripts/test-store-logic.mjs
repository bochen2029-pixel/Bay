// Ad-hoc smoke test for the store's pure helpers. No test-framework
// dependency; run with `node scripts/test-store-logic.mjs`. Exits
// non-zero on failure. Not part of the app runtime.
//
// Pure-logic functions are re-implemented here rather than imported
// from .ts source — keeps the smoke test Node-runnable without a
// transpiler. When either implementation changes, update both.
//
// A real frontend test harness (vitest or similar) lands post-I-14
// when the UI surface stabilizes.

// ─── compareRank / insertByRank / bucketByTier (I-04) ────────────────────────

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

// ─── rankBetween (I-06; mirrors src/rank.ts) ─────────────────────────────────

const DIGITS = "0123456789abcdefghijklmnopqrstuvwxyz";

function digitIndex(c) {
  const i = DIGITS.indexOf(c);
  if (i < 0) throw new Error(`invalid rank digit: ${JSON.stringify(c)}`);
  return i;
}

function midpoint(a, b) {
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
      return b.substring(0, n) + midpoint(aTail, bTail);
    }
  }
  const da = a.length === 0 ? 0 : digitIndex(a[0]);
  const db = b === null ? DIGITS.length : digitIndex(b[0]);
  if (db > da + 1) {
    return DIGITS[Math.floor((da + db) / 2)];
  }
  if (b !== null && b.length > 1) return b.substring(0, 1);
  const head = a.length === 0 ? DIGITS[0] : a[0];
  const tail = a.length === 0 ? "" : a.substring(1);
  return head + midpoint(tail, null);
}

function rankBetween(a, b) {
  if (a !== null && b !== null && !(a < b)) {
    throw new Error(`rankBetween: expected a < b, got a=${JSON.stringify(a)} b=${JSON.stringify(b)}`);
  }
  return midpoint(a ?? "", b);
}

// ─── needsSwap (I-07; mirrors src/swap.ts) ───────────────────────────────────
// The three-way conjunction that distinguishes SwapModal from
// MoveReasonModal at cross-tier drop time.

const A_CAP_JS = 5;
const B_CAP_JS = 12;

function needsSwap(item, targetTier, targetActiveCount) {
  if (targetTier !== "A" && targetTier !== "B") return false;
  if (item.state !== "active") return false;
  const cap = targetTier === "A" ? A_CAP_JS : B_CAP_JS;
  return targetActiveCount >= cap;
}

// ─── onItemUpdated reducer (I-06; mirrors src/store.ts) ──────────────────────

function reduceItemUpdated(state, item) {
  const existing = state.items[item.id];
  if (!existing) return state; // unknown id
  if (existing.updated_at === item.updated_at) return state; // idempotent

  const newItems = { ...state.items, [item.id]: item };
  const newByTier = { ...state.itemsByTier };
  newByTier[existing.tier] = state.itemsByTier[existing.tier].filter(
    (id) => id !== item.id,
  );
  newByTier[item.tier] = insertByRank(newByTier[item.tier], item.id, newItems);
  return { items: newItems, itemsByTier: newByTier };
}

// ─── test harness ────────────────────────────────────────────────────────────

let failures = 0;
function assertEq(actual, expected, label) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) console.log(`  ok  ${label}`);
  else {
    console.error(`  FAIL ${label}\n    actual:   ${a}\n    expected: ${e}`);
    failures++;
  }
}
function assert(cond, label) {
  if (cond) console.log(`  ok  ${label}`);
  else {
    console.error(`  FAIL ${label}`);
    failures++;
  }
}

// ─── compareRank ─────────────────────────────────────────────────────────────

assertEq(compareRank("a", "b"), -1, "compareRank a < b");
assertEq(compareRank("b", "a"), 1, "compareRank b > a");
assertEq(compareRank("m", "m"), 0, "compareRank equal");
assertEq(compareRank("ab", "b"), -1, "compareRank ab < b");
assertEq(compareRank("a", "a0"), -1, "compareRank prefix shorter first");

// ─── insertByRank ────────────────────────────────────────────────────────────

{
  const map = { x: { id: "x", rank: "m" } };
  assertEq(insertByRank([], "x", map), ["x"], "insert into empty");
}
{
  const map = {
    a: { id: "a", rank: "a" },
    b: { id: "b", rank: "b" },
    c: { id: "c", rank: "c" },
  };
  assertEq(insertByRank(["a", "b"], "c", map), ["a", "b", "c"], "append at end");
}
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
{
  const map = {
    a: { id: "a", rank: "a" },
    c: { id: "c", rank: "c" },
    mid: { id: "mid", rank: "b" },
  };
  assertEq(insertByRank(["a", "c"], "mid", map), ["a", "mid", "c"], "insert in middle");
}

// ─── bucketByTier ────────────────────────────────────────────────────────────

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

// ─── ordering invariant under repeated inserts ───────────────────────────────

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

// ─── rankBetween ─────────────────────────────────────────────────────────────

{
  const r = rankBetween(null, null);
  assert(r.length > 0, "rankBetween(null,null) non-empty");
}
{
  const r = rankBetween(null, "m");
  assert(r < "m" && r.length > 0, `rankBetween(null,"m") => ${r} must be < "m"`);
}
{
  const r = rankBetween("a", null);
  assert(r > "a", `rankBetween("a",null) => ${r} must be > "a"`);
}
{
  const r = rankBetween("a", "c");
  assert(r > "a" && r < "c", `rankBetween("a","c") => ${r} must be in (a,c)`);
}
{
  const r = rankBetween("a", "b");
  assert(r > "a" && r < "b", `rankBetween("a","b") => ${r} must be in (a,b)`);
  assert(r.length >= 2, `rankBetween("a","b") extends => ${r} length ≥ 2`);
}
{
  // Repeated inserts between two adjacent keys must stay strictly ordered.
  let lo = "a";
  let hi = "c";
  const seen = [];
  for (let i = 0; i < 10; i++) {
    const r = rankBetween(lo, hi);
    assert(r > lo && r < hi, `iter ${i}: ${r} in (${lo}, ${hi})`);
    seen.push(r);
    hi = r;
  }
}

// ─── onItemUpdated reducer ───────────────────────────────────────────────────

function state1() {
  const items = {
    x: { id: "x", tier: "A", rank: "m", updated_at: 100 },
    y: { id: "y", tier: "A", rank: "p", updated_at: 100 },
    z: { id: "z", tier: "B", rank: "n", updated_at: 100 },
  };
  const itemsByTier = { inbox: [], A: ["x", "y"], B: ["z"], C: [] };
  return { items, itemsByTier };
}

{
  // Unknown id → no change.
  const s = state1();
  const next = reduceItemUpdated(s, {
    id: "nope",
    tier: "A",
    rank: "z",
    updated_at: 200,
  });
  assert(next === s, "onItemUpdated unknown id is identity");
}
{
  // Same updated_at → idempotent no-op.
  const s = state1();
  const next = reduceItemUpdated(s, {
    id: "x",
    tier: "A",
    rank: "something-else",
    updated_at: 100,
  });
  assert(next === s, "onItemUpdated same updated_at is identity");
}
{
  // Intra-tier reorder: x was at rank "m", now "z" (after y).
  const s = state1();
  const next = reduceItemUpdated(s, {
    id: "x",
    tier: "A",
    rank: "z",
    updated_at: 200,
  });
  assertEq(next.itemsByTier.A, ["y", "x"], "intra-tier reorder resorts bucket");
  assertEq(next.items.x.rank, "z", "intra-tier reorder updates item rank");
  assertEq(next.itemsByTier.B, ["z"], "other tier bucket untouched reference");
}
{
  // Cross-tier move: z moves from B to A, lands between x(m) and y(p) at "n".
  const s = state1();
  const next = reduceItemUpdated(s, {
    id: "z",
    tier: "A",
    rank: "n",
    updated_at: 200,
  });
  assertEq(next.itemsByTier.A, ["x", "z", "y"], "cross-tier destination sorted");
  assertEq(next.itemsByTier.B, [], "cross-tier source drained");
  assertEq(next.items.z.tier, "A", "cross-tier item tier updated");
}
{
  // Updating rank to front of tier places at index 0.
  const s = state1();
  const next = reduceItemUpdated(s, {
    id: "y",
    tier: "A",
    rank: "a",
    updated_at: 200,
  });
  assertEq(next.itemsByTier.A, ["y", "x"], "rank to front moves to index 0");
}

// ─── onItemUpdated with sessionDoneIds tracking (I-08) ───────────────────────

function reduceItemUpdatedWithDone(state, item) {
  const existing = state.items[item.id];
  if (!existing) return state;
  if (existing.updated_at === item.updated_at) return state;
  const newItems = { ...state.items, [item.id]: item };
  const newByTier = { ...state.itemsByTier };
  newByTier[existing.tier] = state.itemsByTier[existing.tier].filter(
    (id) => id !== item.id,
  );
  newByTier[item.tier] = insertByRank(newByTier[item.tier], item.id, newItems);
  let newSessionDone = state.sessionDoneIds;
  if (
    item.state === "done" &&
    existing.state !== "done" &&
    !state.sessionDoneIds.has(item.id)
  ) {
    newSessionDone = new Set(state.sessionDoneIds);
    newSessionDone.add(item.id);
  }
  return {
    items: newItems,
    itemsByTier: newByTier,
    sessionDoneIds: newSessionDone,
  };
}

function reduceItemDeleted(state, id) {
  const existing = state.items[id];
  if (!existing) return state;
  const newItems = { ...state.items };
  delete newItems[id];
  const newByTier = { ...state.itemsByTier };
  newByTier[existing.tier] = state.itemsByTier[existing.tier].filter(
    (x) => x !== id,
  );
  return { items: newItems, itemsByTier: newByTier };
}

// ─── needsSwap ───────────────────────────────────────────────────────────────

const activeItem = { id: "x", state: "active" };
const blockedItem = { id: "x", state: "blocked" };
const doneItem = { id: "x", state: "done" };

// Target tier + at-cap + active → swap.
assertEq(needsSwap(activeItem, "A", 5), true, "needsSwap: A active at cap");
assertEq(needsSwap(activeItem, "B", 12), true, "needsSwap: B active at cap");

// Target tier + at-cap + NOT active → MoveReasonModal, not swap.
assertEq(needsSwap(blockedItem, "A", 5), false, "needsSwap: A blocked → no swap");
assertEq(needsSwap(doneItem, "A", 5), false, "needsSwap: A done → no swap");

// Active + target tier but under cap → no swap (regular move).
assertEq(needsSwap(activeItem, "A", 4), false, "needsSwap: A active under cap");
assertEq(needsSwap(activeItem, "B", 11), false, "needsSwap: B active under cap");

// Target is uncapped (Inbox / C) — never a swap.
assertEq(needsSwap(activeItem, "inbox", 999), false, "needsSwap: Inbox never");
assertEq(needsSwap(activeItem, "C", 999), false, "needsSwap: C never");

// Edge: active count above cap (defensive) still triggers swap — the
// UX presumption is "user is trying to add more than allowed".
assertEq(needsSwap(activeItem, "A", 6), true, "needsSwap: A over-cap still swaps");

// ─── onItemUpdated sessionDoneIds tracking (I-08) ────────────────────────────

function state2() {
  return {
    items: {
      x: { id: "x", tier: "A", rank: "m", state: "active", updated_at: 100 },
      y: { id: "y", tier: "A", rank: "p", state: "active", updated_at: 100 },
    },
    itemsByTier: { inbox: [], A: ["x", "y"], B: [], C: [] },
    sessionDoneIds: new Set(),
  };
}

{
  const s = state2();
  const next = reduceItemUpdatedWithDone(s, {
    id: "x",
    tier: "A",
    rank: "m",
    state: "done",
    updated_at: 200,
  });
  assert(next.sessionDoneIds.has("x"), "session-done: X tracked when state→done");
  assert(next.items.x.state === "done", "session-done: projection updated");
}
{
  // Non-done→done-but-already-tracked: no duplicate-add churn on Set
  const s = state2();
  s.sessionDoneIds = new Set(["x"]);
  const next = reduceItemUpdatedWithDone(s, {
    id: "x",
    tier: "A",
    rank: "m",
    state: "done",
    updated_at: 200,
  });
  assert(
    next.sessionDoneIds === s.sessionDoneIds,
    "session-done: already-tracked skips Set allocation",
  );
}
{
  // Active→blocked: not done, no tracking
  const s = state2();
  const next = reduceItemUpdatedWithDone(s, {
    id: "x",
    tier: "A",
    rank: "m",
    state: "blocked",
    blocked_reason: "waiting",
    updated_at: 200,
  });
  assertEq(
    [...next.sessionDoneIds],
    [],
    "session-done: active→blocked doesn't track",
  );
}

// ─── onItemDeleted ───────────────────────────────────────────────────────────

{
  const s = {
    items: {
      x: { id: "x", tier: "A", rank: "m", state: "active", updated_at: 100 },
      y: { id: "y", tier: "A", rank: "p", state: "active", updated_at: 100 },
    },
    itemsByTier: { inbox: [], A: ["x", "y"], B: [], C: [] },
  };
  const next = reduceItemDeleted(s, "x");
  assert(!("x" in next.items), "onItemDeleted: id removed from items map");
  assertEq(next.itemsByTier.A, ["y"], "onItemDeleted: id removed from tier bucket");
}
{
  // Unknown id → identity
  const s = {
    items: {},
    itemsByTier: { inbox: [], A: [], B: [], C: [] },
  };
  const next = reduceItemDeleted(s, "nope");
  assert(next === s, "onItemDeleted: unknown id is identity");
}

if (failures > 0) {
  console.error(`\n${failures} failure(s)`);
  process.exit(1);
}
console.log("\nall store-logic checks passed");
