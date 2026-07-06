//! Attach a free-form breadcrumb to any error: [`Context`].

use alloc::string::String;

use crate::Error;

/// Attach a free-form context string to any error.
///
/// Use this when the inner error on its own doesn't carry enough
/// information to localise *where* the failure happened.  The
/// `context` field is a human-readable breadcrumb — a field name, an
/// argument index, the `Debug` rendering of the offending input —
/// and `source` chains through [`core::error::Error::source`] so
/// callers can still match on the underlying cause.
///
/// `Context` nests naturally: wrap a `Context<E>` in another
/// `Context<Context<E>>` to accumulate breadcrumbs on the way up.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{context}: {source}")]
pub struct Context<E: core::error::Error + 'static> {
    /// Breadcrumb describing where `source` arose.
    pub context: String,
    /// The underlying error.
    #[source]
    pub source: E,
}

impl<E: core::error::Error + 'static> Context<E> {
    /// Wrap `source` with a breadcrumb.
    pub fn new(context: impl Into<String>, source: E) -> Self {
        Self { context: context.into(), source }
    }
}
