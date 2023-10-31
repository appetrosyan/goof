# Goof

An early composable, and re-usable library for tiny structs that you
would be writing on your own, but which you really shouldn\'t.

``` rust

use goof::{Mismatch, assert_eq};

fn fallible_func(thing: &[u8]) -> Result<(), Mismatch<usize>> {
    assert_eq(&32, &thing.len())?;

    Ok(())
}

assert_eq!(fallible_func(&[]).unwrap_err(), assert_eq(&32, &0).unwrap_err())
```

So why use it? It\'s pre-alpha, so it\'s not particularly useful yet.
But imagine a situation in which you don\'t really want to panic on
failed assertions. These functions can then be a lightweight 1-1
replacement of the standard library \`assert~eq~!\` macro. It will not
panic immediately, but instead create a structure called
rust~src~{Mismatch}, which has all of the appropriate traits (except
`std::error::Error`{.verbatim} because I want to add `std`{.verbatim}
support, rather than subtract it) implemented.

These will participate in all manner of goodies, that don\'t necessarily
depend on `std`{.verbatim}, but that can effectively make
`goof`{.verbatim} a one-stop-shop for all your error handling needs. It
will hopefully be as useful as `eyre`{.verbatim} and
`thiserror`{.verbatim} while providing a slightly different approach to
designing error APIs.

# Goals

### Overarching API decisions

-   Be `no_std`{.verbatim} compatible. The structures should be easy to
    use across the FFI boundary, simple and hopefully predictable in
    their behaviours.
-   Embedded-friendly. We want to act as if we have an allocator, while
    keeping all of the structures on-stack for as much as possible.
-   All structs should attempt to be `Copy`{.verbatim}-able if possible.
-   Nudge users to avoid panics as much as possible.
-   Ergonomic design. Commonly used patterns should be terse and
    maximally easy to write.
-   Few to no dependencies.
-   Few to no features.
-   LTO-friendly code elimination.

### Supported use-cases

-   Contextual error propagation like the old
    [`failure`{.verbatim}](https://docs.rs/failure/latest/failure/)
    crate.
    -   Each error can have an optional wrapping structure which
        explains the context and helps in debugging.
    -   Support for [tracing
        spans](https://docs.rs/tracing/latest/tracing/), so that errors
        have tracing spans attached.
-   Different error handling methods:
    -   Fail fast, where any failed assertion immediately produces the
        corresponding error and is being propagated upwards.
    -   Fail completely, where any failure will stop some logic from
        being executed, but will accumulate errors instead of
        immediately propagating them upwards.
    -   Fail recoverably, where functions to try and catch specific
        failure modes can be applied to recover from some, but not all
        error conditions.
    -   Resumable error, where any form of failure is propagated up the
        call stack, but the failure can be corrected and the function
        can be resumed.
-   Pretty printing, like in `eyre`{.verbatim}, and (hopefully) like in
    `miette`{.verbatim}.

# Progress

The library is in its early stages. I\'m planning on approaching this
from the minimalist perspective, of making a bunch of `0.*`{.verbatim}
versions and when the library is complete, releasing the
`1.0`{.verbatim} version. While this is in no way a pre-release
candidate and as is, it should be ready for production use, I would
recommend not spending too much time worrying about the changes in the
newer versions. Update as you see fit, if you do, I will be providing
detailed notes on how to make the jump.

# Changelog

-   0.1.0
    -   Initial, extremely basic implementation of
        `Mismatch`{.verbatim}, `Outside`{.verbatim} and
        `Unknown`{.verbatim} structures.
    -   Initial implementations of `assert_eq`{.verbatim},
        `assert_in`{.verbatim}, `assert_known_enum`{.verbatim}, and
        `assert_known`{.verbatim}.
-   0.2.0
    -   Swapped around arguments in `assert_eq`{.verbatim} for more
        consistency.
