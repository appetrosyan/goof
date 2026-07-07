# LLMs in `goof` — policy & philosophy

**Status: active policy. The end state is a codebase with no LLM-authored code left in it.**

## The stance

Large language models are a prototyping tool, not a substitute for a person. In
this project they are allowed to help sketch, scaffold, and explore — the same
way you might spike a design in a throwaway branch — but nothing an LLM writes is
considered finished. Generated code earns its place only once a human has
understood it, rewritten it, and would defend every line of it unprompted.

The reasoning is not ideological. Generated code in this repo has consistently
shown the failure modes you'd expect: near-duplicate functions a person would
have factored, defensive handling of cases that never arise, docs written in a
reference-manual register nobody actually speaks, and subtle contamination of
otherwise-human code (a trait bound listed twice, a `panic!` traded for silent
divergence). It prototypes quickly and maintains poorly. For a small library
whose whole pitch is "tiny, predictable, `no_std`, few dependencies," that trade
is backwards.

So the goal is a **proudly LLM-free `goof`**: every shipped line hand-crafted,
`git blame` free of generated commits, and the design owned end to end by the
people whose names are on it.

## What this is not

This is not a ban on using the tools. Prototyping with an LLM is fine and
sometimes useful. The line is at what *ships*: an LLM may help you find the
shape of a solution; it may not be the author of record for code that stays.
Treat its output like a stranger's pull request you're on the hook to maintain —
usually easier to rewrite than to review into shape.

## Provenance, as of this writing

The library has a clear seam. The original core (2023) is hand-written; a large
LLM-generated layer landed in 2026, beginning with commit `f1e34a3`
("Rip off thiserror", 2026-03-07), which introduced a ~1000-line proc-macro
whole in a single commit. Real hand-crafting is interleaved with that layer, so
provenance is tracked per component, not per file — and note that the module
files were only *split out* in 2026-07-06; several descend from the 2023 core.

**Human-origin, essentially intact (keep):**
- `Mismatch` and its `Display`/`Debug` impls — verbatim from the 2023 original,
  bar an LLM-introduced duplicate trait bound to undo.
- The `assert_*` ergonomics, the "everything `Copy` where possible" philosophy,
  and the `README.org` voice and Goals.

**Human concept, LLM-rewritten implementation (rewrite the body, keep the design):**
- `Outside` — the `RangeBounds`/`Bound` reimplementation of the 2023 range check.
- `Unknown` — the allocation-free reimplementation of the 2023 known-set check.
- The crate- and module-level doc comments.

**LLM-authored, no human predecessor (rewrite from scratch; nothing human to lose):**
- The entire `goof-derive` proc-macro crate.
- `Errors` (`src/aggregate.rs`).
- `Mishap`, the `Goof` trait, and `Context`.
- The integration tests in `tests/derive.rs`.

## The goal, concretely

`goof` is LLM-free when:
1. No component in the "LLM-authored" list above survives in generated form.
2. The "LLM-rewritten" bodies have been reimplemented by hand, keeping their
   human-origin designs.
3. The stray contamination in the human-origin code is cleaned up.
4. `git blame -w -M -C` over `src/` and `goof-derive/src/` points at no
   2026-era generated commit for any line that ships.

Until then, this file is the standard new work is held to: prototype however you
like, but hand-craft what lands.

## Verifying provenance

To date any block of code rather than guess:
- `git show 9659526:src/lib.rs` — the 2023 human original; match against it.
- `git blame -w -M -C <file>` — follows content across the 2026-07-06 file split,
  so blame points at where a line was really written, not where the file was made.
- `git log --shortstat` — any single 2026-era commit adding >300 lines to one
  file is almost certainly generated.
