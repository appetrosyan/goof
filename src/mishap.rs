//! A type-erased, `Arc`-backed wrapper around any `dyn Error`: [`Mishap`].

use alloc::sync::Arc;
use core::fmt::{Debug, Display};

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
/// ```rust
/// use goof::Error;
///
/// #[derive(Debug, Error)]
/// pub enum AppError {
///     #[error("Failed to set mpv option {option}: {detail}")]
///    MpvOption { option: String, detail: String },
/// }
/// ```
///
#[derive(Clone)]
pub struct Mishap {
    wrapped_err: Arc<dyn core::error::Error>,
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

impl core::error::Error for Mishap {}
