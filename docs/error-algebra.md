# Error Algebra & C-ABI Interchange — design note

**Status: designed, deliberately OUT OF SCOPE for the current implementation.**
This records a design we worked out and then chose *not* to build yet. It is
here so the reasoning is not lost. Do not start on it until (a) the ergonomic
layer has shipped and (b) the corpus survey below has been run — freezing the
node set is the one irreversible step, and it must be evidence-based.

## Motivation

Rust ↔ C-ABI-`cdylib` ↔ Rust (or C, or another language) is avoided in practice
because even the most basic thing — error handling — is miserable across that
seam, so people flatten errors to an `int` and give up. The aim is to pave that
gap: because `goof` is already your `thiserror`/`anyhow` layer, crossing the
boundary should be a *consequence* of handling errors normally, not a second
serialization stack you hand-wire.

A good error must deliver two things: **provenance** (what invariant broke, over
which inputs *and* machine state) and **recoverability** (the data a handler
needs to substitute and retry).

## Thesis: errors are broken invariants, and the shapes are enumerable

Most domain-independent errors are "an invariant I promised was violated," and
invariants come in a small set of shapes. Crucially, most of them *collapse*:

- `not-found`, `Outside(range)`, `Unknown(set)`, wrong-enum-variant, `Duplicate`,
  `Capacity`, `Timeout`, `Overflow`, `State` all reduce to **membership**
  (`x ∈ S`) distinguished only by (a) how `S` is represented and (b) a polarity
  bit (required vs forbidden). `Overflow(a+b)` = `a+b ∈ representable`;
  `not-found` = membership with `S` unspecified; `Duplicate` = polarity flipped.
- `Mismatch` is a **relation** (`x R y`, `R = Eq`). It is embeddable as
  `x ∈ {y}` but kept separate — the `+` vs `×` situation: distinct generators of
  a free algebra even when one encodes the other, because the peer-symmetry of a
  relation is lost by the singleton-set encoding.

So the *semantic* zoo is large but the *structural* core is tiny.

## Two layers — this split is the whole payoff

- **Structural generators** — few, frozen, and the wire ABI.
- **Semantic constructors** — many, growable, each defined by *how it lowers* to
  the structural generators.

Because `Timeout`/`Duplicate`/`Overflow`/… lower to already-frozen nodes, new
ergonomic types can be added **without an ABI break**. We freeze ~5 algebraic
node kinds, never the semantic list.

## The structural core (five nodes)

```
Error = μX. F(X)                         -- initial algebra of a polynomial functor
F(X) = Violation                         -- a broken invariant, carrying its collective
     | Record                            -- a semantically-uninterpreted structured leaf
     | Opaque                            -- a fully-erased foreign error (bytes/handle/display)
     | Context(Message, X)               -- a breadcrumb over a child
     | Aggregate([X])                    -- several children combined

Violation { kind, participants: [Value], constraint }
    kind       = Relation(op)            -- Mismatch = Relation(Eq); participants = (x, y)
               | Membership(SetRepr, polarity)
    SetRepr    = Unspecified | Ranges(..) | Enumerated(..) | Variants(..)
               | Derived { name, witness: Value }   -- set defined by state; carry the state
```

## The collective (the part everyone gets wrong)

The witness of a violation is a **tuple of participants (inputs + relevant
state)**, not a scalar. `Overflow` carries `(a, b)`; file-not-found carries
`(filename, path)` — because `files(path)` is the set, and "not found *where*"
is answered by `Derived { "files", witness: path }`. We never enumerate the
concrete set; we ship a **sound over-approximation** (the state that generated
it) — abstract interpretation, not the whole directory.

The collective is **double-duty**: it is simultaneously the provenance *and* the
recovery payload.

- Python-style: gives the context chain but not the operands (provenance, no data).
- Rust-style: lets you interrupt and substitute anywhere, but you hand-carry the
  data (clone inputs "just in case", big `match`, retry), decoupling in-code
  structure from semantic structure.
- `goof`: the `Violation` node *is* the collective, so `Context` gives the chain
  *and* each node already holds the values a handler needs. You stop hand-carrying
  clones because the error is the saved context.

Only the **site** knows its collective, which is why invariants must be encoded
at the type/function (via constructors) rather than lifted into a distant
god-enum. Co-location is a requirement, not a preference.

## Wire format = serialization, not transmute

The wire is a *serialization of the tree*, decoded by a **catamorphism**
(streaming visitor over the five nodes). This is entirely safe — no `unsafe`, no
uninitialised padding crossing, no discriminant UB — and it validates on decode,
so `bool`/`char`/strings ride along and cross-platform canonicalisation
(little-endian, fixed widths) is available. `#[repr(C)]` demotes from *the
contract* to a mere convenience.

The far end needs **only `goof`'s five-node vocabulary**, never the producer's
types: it decodes a `Violation`/`Context`/`Aggregate` tree, renders it, walks it,
and reads a buried stable link's participants — all type-free. That dissolves the
"unknown types" limitation. Structured downcast to a concrete user type is *also*
available when both ends happen to share it (via stable tags).

### Why not just `serde`?

`serde` is type-coupled and manual (annotate, manage variant identity,
explicitly deserialise into shared types), has **no frozen C-shaped fixed-size
schema** (bincode's layout moved between 1.x/2.0; postcard is variable-length and
Rust-oriented), and pulling `serde_derive` drags `syn` back — the very thing
`goof-derive` exists to avoid. For **Rust↔Rust with shared types**, `serde` +
`postcard` is genuinely the better answer and the docs should say so. The
bespoke format earns its keep only for the **C / no-shared-types / frozen-header**
case and for the **universal error algebra** (type-free decode), which `serde`
structurally cannot provide.

## Lowering and decoding

```
trait Lower { fn lower(&self, sink: &mut dyn ErrorSink); }
```

Each type lowers itself into nodes: `Mismatch → Relation(Eq)`,
`Outside`/`Unknown` → `Membership(..)`, `Context` → `Context`, `Errors` →
`Aggregate`, `Mishap` → `Opaque`, a user `derive` → `Record`. Decode is one
generic catamorphism; the consumer supplies per-node handlers.

## ABI bridge & runtime discovery (later layer)

An `extern "C"` family per catalog exports `display`/`source`/`drop`/… ; a far
Rust side reconstructs a `ForeignError` from the exported table (obtained via
`dlsym` or a passed pointer — **no C header required**) and folds it. `Opaque`
nodes cross as owned handles you can still render and walk.

`ErrorCatalog` is **demoted**: it is an opt-in registry for stable tags, typed
exhaustiveness, and crossing — *not* the required vehicle. The self-describing
algebra is the default, so the god-enum becomes a **choice**, taken only where
compile-time exhaustiveness earns its cost.

## Recovery

The collective is the retry payload; expose a `recover` / `or_else_with`
combinator over it. True resume-in-place needs delimited continuations Rust does
not have, so this is *data-for-retry*, not resumption. Categorically it is a
partial catamorphism that replaces a `Violation` subtree with a recomputed value.

## Semantic / ergonomic layer (all lowers to the five nodes)

`assert_*` family, `ensure!`/`bail!`, an enum-membership derive, a
`goof::arithmetic! { a + b*c - d }` DSL (checked arithmetic → `Membership`/
`Relation` over the operands, never a new node), `#[goof::fallible]` function-local
enum synthesis, and `Missing`/`Duplicate`/`Timeout`/`State` constructors. Which of
these ship, and in what form, is an output of the survey — not frozen here.

## Corpus survey (run before freezing anything)

Grep a corpus (std, tokio, hyper, serde, rustc, several app codebases) for
`enum \w*Error` and `struct \w*Error`; bucket every *variant* by invariant shape;
count frequencies. This tests the thesis (a handful of shapes should cover the
tail), tells us the primitive vs leaf split, and finalises the node set and the
`SetRepr` vocabulary with evidence.

## Risks (why this is deferred)

- **Over-engineering / never shipping** — the reason this sits behind the
  ergonomic wins rather than blocking them.
- **Premature freeze** — the node set is an ABI; freeze only after the survey.
- **Capture cost / which-state is undecidable** — the site must declare its
  participants and capture a bounded, sound abstraction.
- **Resume needs continuations** — out of reach; we offer retry-with-data only.
