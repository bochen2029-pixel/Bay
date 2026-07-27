#!/usr/bin/env python3
"""
CI check: every registered Tauri command is reachable from the app.

A command in `generate_handler!` that nothing ever `invoke()`s is dead
surface — and worse, it is usually a HALF-SHIPPED FEATURE rather than
dead code. Two were found this way in v0.3:

  * `set_first_step` — registered, unit-tested, and rendered in three
    places, with the Mirror reporting "no first step" as an avoidance
    signal, while nothing in the UI could set one.
  * `add_to_today`  — the only route onto Today was the day-open
    picker; the obvious per-item gesture had no affordance.

Both had passing backend tests. This is the same failure shape as a
golden case that is counted but never executed, one layer up: the
thing exists, and nothing exercises it.

Usage:
  python scripts/check-reachability.py

Exits non-zero if a registered command has no `invoke("name")` call in
`src/` (excluding test files) and is not in ALLOWED_UNREACHABLE.
"""

import os
import re
import sys

REPO_ROOT = os.path.join(os.path.dirname(__file__), "..")
LIB_RS = os.path.join(REPO_ROOT, "src-tauri", "src", "lib.rs")
FRONTEND = os.path.join(REPO_ROOT, "src")

# Commands deliberately not called from the frontend, with the reason.
# Keep this list SHORT and justified — it is the escape hatch, and an
# unexplained entry here is how a half-shipped feature hides.
ALLOWED_UNREACHABLE = {
    "get_settings": (
        "settings arrive via `bootstrap`; the separate read command is "
        "available API, same call as DECISIONS ADR-005 for lanCapture"
    ),
}


def registered_commands() -> list[str]:
    """Command names inside the `generate_handler![...]` block."""
    with open(LIB_RS, "r", encoding="utf-8") as f:
        src = f.read()
    match = re.search(r"generate_handler!\s*\[(.*?)\]", src, re.DOTALL)
    if not match:
        print(f"FAIL: no generate_handler! block found in {LIB_RS}", file=sys.stderr)
        sys.exit(2)
    body = re.sub(r"//[^\n]*", "", match.group(1))
    # Entries look like `bootstrap,` or `commands::items::create_item,`.
    return [
        seg.strip().split("::")[-1]
        for seg in body.split(",")
        if seg.strip()
    ]


def frontend_sources() -> str:
    """All non-test frontend source, concatenated."""
    chunks = []
    for root, _dirs, files in os.walk(FRONTEND):
        for name in files:
            if not name.endswith((".ts", ".tsx")):
                continue
            if ".test." in name:
                continue  # a test-only call proves nothing about the app
            with open(os.path.join(root, name), "r", encoding="utf-8") as f:
                chunks.append(f.read())
    return "\n".join(chunks)


def main() -> int:
    commands = registered_commands()
    src = frontend_sources()

    unreachable = []
    for cmd in commands:
        # Match the command as a quoted string, which is how `invoke`
        # names it. Deliberately loose about the call shape (invoke,
        # a wrapper, a constant) — presence of the literal is enough.
        if re.search(rf"""["'`]{re.escape(cmd)}["'`]""", src):
            continue
        if cmd in ALLOWED_UNREACHABLE:
            print(f"OK  {cmd} (allowed: {ALLOWED_UNREACHABLE[cmd]})")
            continue
        unreachable.append(cmd)

    stale = sorted(set(ALLOWED_UNREACHABLE) - set(commands))
    for cmd in stale:
        print(f"NOTE: ALLOWED_UNREACHABLE lists {cmd!r}, which is no longer registered")

    if unreachable:
        print()
        for cmd in unreachable:
            print(
                f"FAIL: `{cmd}` is registered in generate_handler! but never invoked "
                f"from src/. Either wire it into the UI, or justify it in "
                f"ALLOWED_UNREACHABLE."
            )
        print(f"\n{len(unreachable)} unreachable command(s)")
        return 1

    print(f"\nAll {len(commands)} registered commands are reachable.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
