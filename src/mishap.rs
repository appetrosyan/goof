//! A type-erased, `Arc`-backed wrapper around any `dyn Error`: [`Mishap`].

use alloc::sync::Arc;
use core::fmt::{Debug, Display};

use crate::Context;

/// This type is a rough equivalent of [`eyre::Report`](), with a few key differences.
///
/// There are often situations in which it is not possible to tell if
/// some type is a specific kind of error. This is where you would
/// normally use a `Box<dyn Error>` and make the life of every
/// downstream consumer of your code miserable.
///
/// A good case in point is [`std::io::Error`].  It contains a
/// `Box<dyn Error>` without a `Clone` bound. You can't add one
/// yourself, and by the time you receive the error, not much can be
/// done.  A mishap is a much more forgiving version of that.  True,
/// the overhead of an `Arc` is big. As is the overhead of going with
/// a trait object.  Quite honestly, your errors should be on a cold
/// path anyway.
///
/// Unlike [`eyre::Report`](), `Mishap` *keeps* its
/// [`Error`](core::error::Error) implementation, so it still nests
/// cleanly as a `#[source]` / `#[from]` field.  The typing that a
/// blanket `From` would save is instead recovered through the
/// [`Goof`] extension trait:
///
/// ```rust
/// use goof::{Goof, Mishap};
///
/// fn parse_retries(raw: &str) -> Result<i32, Mishap> {
///     // `.goof` attaches a breadcrumb and erases the concrete error.
///     let n: i32 = raw.trim().parse().goof("parsing the retry count")?;
///     Ok(n)
/// }
///
/// let err = parse_retries("not a number").unwrap_err();
/// assert!(err.to_string().starts_with("parsing the retry count: "));
/// // The original `ParseIntError` is still reachable through the chain.
/// assert!(core::error::Error::source(&err).is_some());
/// ```
///
/// For a bare erasure without a breadcrumb, use [`Mishap::new`].
#[derive(Clone)]
pub struct Mishap {
    wrapped_err: Arc<dyn core::error::Error>,
}

impl Mishap {
    /// Type-erase any error into a `Mishap`.
    ///
    /// To attach a contextual breadcrumb at the same time, prefer the
    /// [`Goof::goof`] extension method on `Result`.
    pub fn new(err: impl core::error::Error + 'static) -> Self {
        Self { wrapped_err: Arc::new(err) }
    }
}

impl Display for Mishap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.wrapped_err)
    }
}

impl Debug for Mishap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Mishap")
            .field("wrapped_err", &self.wrapped_err)
            .finish()
    }
}

impl core::error::Error for Mishap {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        // `Mishap` presents the wrapped error transparently: its own
        // `Display` already renders the inner error, so the chain
        // continues from the inner error's *own* source to avoid
        // rendering the same message twice.
        self.wrapped_err.source()
    }
}

/// Attach a breadcrumb to an error and type-erase it into a [`Mishap`].
///
/// This is the ergonomic entry point for the `Mishap` catch-all: it
/// replaces the blanket `From<E: Error>` conversion that `Mishap`
/// cannot offer (that would collide with its own `Error` impl).  The
/// breadcrumb is carried by [`Context`], so the concrete error type
/// stays reachable through [`Error::source`](core::error::Error::source).
///
/// ```rust
/// use goof::{Goof, Mishap};
///
/// let opened: Result<(), Mishap> =
///     std::fs::read_to_string("/no/such/file")
///         .map(|_| ())
///         .goof("loading the configuration");
/// assert!(opened.is_err());
/// ```
pub trait Goof<T> {
    /// Wrap the error with `context` and erase it into a [`Mishap`].
    fn goof(self, context: &str) -> Result<T, Mishap>;
}

impl<T, E: core::error::Error + 'static> Goof<T> for Result<T, E> {
    fn goof(self, context: &str) -> Result<T, Mishap> {
        self.map_err(|err| Mishap::new(Context::new(context, err)))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Goof, Mishap};

    #[derive(Debug)]
    struct Boom;

    impl core::fmt::Display for Boom {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("boom")
        }
    }

    impl core::error::Error for Boom {}

    #[test]
    fn new_forwards_display() {
        let mishap = Mishap::new(Boom);
        assert_eq!(mishap.to_string(), "boom");
    }

    #[test]
    fn new_erased_error_has_no_extra_source() {
        // `Boom` has no source, so the transparent forwarding yields none.
        let mishap = Mishap::new(Boom);
        assert!(core::error::Error::source(&mishap).is_none());
    }

    #[test]
    fn goof_attaches_context_and_chains_source() {
        let result: Result<(), Boom> = Err(Boom);
        let mishap = result.goof("while defusing").unwrap_err();
        assert_eq!(mishap.to_string(), "while defusing: boom");
        // The original `Boom` is reachable through the context layer.
        let source = core::error::Error::source(&mishap).expect("source present");
        assert_eq!(source.to_string(), "boom");
    }

    #[test]
    fn clone_shares_the_inner_error() {
        let mishap = Mishap::new(Boom);
        let clone = mishap.clone();
        assert_eq!(mishap.to_string(), clone.to_string());
    }
}
