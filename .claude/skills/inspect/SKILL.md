---
name: inspect
description: Measure-first discipline. Before reading any resource with a per-input limit (files >20K tokens, images >2000px, large dirs, large responses), inspect size first and choose strategy. Pre-flight inspection > error recovery. The chunker at C:\chunker\ is the large-file sub-skill.
---

# /inspect <resource>

Measure first, consume second. A hard discipline, not opportunistic
optimization. A failed oversized read leaves context degraded —
partial reads, truncation, error tokens — costlier than measuring
first.

## Resource thresholds

| Resource | Measurement | Threshold | Strategy if exceeded |
|---|---|---|---|
| File read | `wc -c` / `wc -l` | ~20K tokens (~80KB English) | Dispatch chunker; CHUNK_SUMMARY; targeted re-reads |
| Image view | dimensions | 2000px any side | Resize to ≤2000px; view resized copy |
| Directory listing | `find <dir> -maxdepth 1 \| wc -l` | ~200 entries | Sample head/tail; grep-filter |
| API response | content-length | ~50K chars | Paginate, stream, summarize |
| Tool output | redirect to file, `wc -c` | ~30K tokens | Truncate, chunked read |
| Git diff | `git diff --stat` first | ~500 lines | Read by file/hunk |
| DB query | `EXPLAIN` first | scan cost / rows | Limit, paginate, aggregate |
| Build/test output | redirect to file | unlimited | tail last N; grep failures |

## The chunker (large-file sub-skill)

At `C:\chunker\chunker.py`. Token-aware document chunker — splits a
file too big to read in one shot into context-sized chunks at clean
semantic boundaries, with orientation metadata.

```bash
# Split a file into <file>.chunks/ (chunk-001.md … + INDEX.md + _manifest.json)
python C:/chunker/chunker.py --budget 100000 "path/to/huge_file.md"

# Just estimate + show the plan, write nothing
python C:/chunker/chunker.py --plan "path/to/huge_file.md"

# Print one chunk to stdout
python C:/chunker/chunker.py --budget 100000 --stdout 3 "path/to/huge_file.md"
```

`--budget` is tokens of content per chunk — set to roughly half your
available context window. Default 100000.

### Workflow

1. Detect size: `wc -c <big_file>` → 800000 bytes (~200K tokens).
2. Invoke chunker: `python C:/chunker/chunker.py <big_file>`.
3. Read `INDEX.md` first — lists every chunk, token count, section.
4. Read specific chunks as needed (each opens with a `section:`
   breadcrumb; from chunk 2 on, a `recap:` quotes the previous chunk's
   tail).
5. `.chunks/` is gitignored; chunks are ephemeral.

## Generalization

The pattern generalizes to any per-input constraint. **Look before
you leap.** The cost of measuring is bounded and low. The cost of
failed consumption is unbounded and high.
