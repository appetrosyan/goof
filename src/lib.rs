//! A reusable, composable, aggregate and `no_std`-friendly error library.
//!
//! `goof` provides two things:
//!
//! 1. **Ready-made error types** — small, generic structs that cover
//!    the most common "something went wrong" shapes:
//!    [`Mismatch`], [`Outside`], [`Unknown`] and the type-erased
//!    [`Mishap`].
//!
//! 2. **A `derive(Error)` macro** — a drop-in replacement for
//!    [`thiserror`](https://docs.rs/thiserror) that generates
//!    [`core::error::Error`], [`core::fmt::Display`] and
//!    [`core::convert::From`] implementations from a compact attribute
//!    syntax.
//!
//! # Quick start
//!
//! ```rust
//! use goof::Error;
//!
//! #[derive(Debug, Error)]
//! pub enum AppError {
//!     #[error("configuration file not found")]
//!     NotFound,
//!     #[error("i/o error")]
//!     Io(#[from] goof::Mishap),
//!     #[error("invalid value {value} (expected {expected})")]
//!     Invalid { value: i32, expected: i32 },
//!		#[error("Failed to set mpv option {option}: {detail}")]
//!     MpvOption { option: String, detail: String },
//! }
//!
//! let e = AppError::NotFound;
//! assert_eq!(e.to_string(), "configuration file not found");
//! ```
//!
//! # The derive macro
//!
//! `#[derive(Error)]` is re-exported from the [`goof-derive`] proc-macro
//! crate and accepts the same attributes as `thiserror`.  The macro
//! works on structs (unit, tuple, or named-field) and enums.
//!
//! ## `#[error("...")]` — Display generation
//!
//! Place the attribute on a struct or on each variant of an enum to
//! generate a [`Display`](core::fmt::Display) implementation.  The
//! format string supports the usual `std::fmt` syntax **plus** the
//! following shorthands for referring to fields of the error value:
//!
//! | Shorthand       | Expands to                    |
//! |-----------------|-------------------------------|
//! | `{var}`         | `self.var` (Display)          |
//! | `{0}`           | `self.0`   (Display)          |
//! | `{var:?}`       | `self.var` (Debug)            |
//! | `{0:?}`         | `self.0`   (Debug)            |
//!
//! Additional format arguments are passed through verbatim.  If an
//! extra argument needs to refer to a field, prefix the field path with
//! a dot:
//!
//! ```rust
//! # use goof::Error;
//! #[derive(Debug, Error)]
//! #[error("invalid rdo_lookahead_frames {0} (expected < {max})", max = i32::MAX)]
//! struct InvalidLookahead(u32);
//! ```
//!
//! If `#[error("...")]` is **not** present, no `Display` impl is
//! generated and the caller must provide one manually.
//!
//! ## `#[error(transparent)]` — forwarding
//!
//! Delegates both `Display` and `Error::source()` to the single inner
//! field.  Useful for newtype wrappers and "catch-all" enum variants.
//!
//! ```rust
//! # use goof::Error;
//! // Opaque public error whose representation can change freely.
//! #[derive(Debug, Error)]
//! #[error(transparent)]
//! pub struct PublicError(#[from] InternalError);
//!
//! #[derive(Debug, Error)]
//! #[error("internal")]
//! pub struct InternalError;
//! ```
//!
//! A second field is allowed only when its type name ends in
//! `Backtrace` (or it carries the `#[backtrace]` attribute).
//!
//! ## `#[source]` — error chaining
//!
//! Marks a field as the underlying cause.  The generated
//! [`Error::source()`](core::error::Error::source) returns it.
//!
//! A field literally named `source` is recognised automatically; the
//! attribute is only needed when the field has a different name.
//!
//! ```rust
//! # use goof::Error;
//! #[derive(Debug, Error)]
//! #[error("operation failed")]
//! struct OpError {
//!     #[source]
//!     cause: std::io::Error,
//! }
//! ```
//!
//! ## `#[from]` — automatic `From` conversion
//!
//! Generates a [`From<T>`](core::convert::From) impl that converts the
//! annotated field's type into the error.  Implies `#[source]`.
//!
//! ```rust
//! # use goof::Error;
//! #[derive(Debug, Error)]
//! enum LoadError {
//!     #[error("i/o error")]
//!     Io(#[from] std::io::Error),
//!     #[error("parse error")]
//!     Parse(#[from] std::num::ParseIntError),
//! }
//!
//! // Now you can use `?` on both io::Error and ParseIntError.
//! fn load(path: &str) -> Result<i32, LoadError> {
//!     let s = std::fs::read_to_string(path)?;
//!     Ok(s.trim().parse()?)
//! }
//! ```
//!
//! The variant using `#[from]` must not contain any other fields beyond
//! the source error (and possibly a backtrace).
//!
//! ## `#[backtrace]` — backtrace capture (nightly)
//!
//! A field whose type name ends in `Backtrace` is automatically
//! detected, or can be explicitly tagged with `#[backtrace]`.  When
//! present, the generated `Error::provide()` exposes it as a
//! [`std::backtrace::Backtrace`].  If a `#[from]` field coexists with
//! a backtrace field, the `From` impl calls `Backtrace::capture()`.
//!
//! If `#[backtrace]` is placed on a source field, `provide()` forwards
//! to the source's own `provide()` implementation so that both layers
//! share the same backtrace.
//!
//! *Requires a nightly compiler; the generated code is behind
//! `#[cfg(error_generic_member_access)]`.*
//!
//! # Ready-made error types
//!
//! These types cover everyday validation patterns and can be used
//! directly as error types or composed into larger error enums.
//!
//! | Type | Meaning |
//! |------|---------|
//! | [`Mismatch<T>`] | An equality check failed (`expected` vs `actual`). |
//! | [`Outside<T>`]   | A value fell outside a required range. |
//! | [`Unknown<T>`]   | A value is not among a known set. |
//! | [`Mishap`]       | A type-erased, `Arc`-backed wrapper around any `dyn Error`. |
//!
//! Each type (except [`Unknown`] and [`Mishap`]) implements
//! [`Error`](core::error::Error) via `derive(Error)`.
//!
//! ## Assertion helpers
//!
//! The library also exposes lightweight assertion functions that return
//! `Result` instead of panicking, making them suitable for use with the
//! `?` operator:
//!
//! | Function | Error type | Purpose |
//! |----------|------------|---------|
//! | [`assert_eq`] | [`Mismatch<T>`] | Two values must be equal. |
//! | [`assert_in`] | [`Outside<T>`]  | A value must lie within a range. |
//! | [`assert_known_enum`] | [`Unknown<T>`] | A value must be in a known slice (returns the slice on error). |
//! | [`assert_known`] | [`Unknown<T>`] | A value must be in a known slice (error omits the slice). |
//!
//! ```rust
//! use goof::{Mismatch, assert_eq};
//!
//! fn check_len(buf: &[u8]) -> Result<(), Mismatch<usize>> {
//!     assert_eq(&buf.len(), &64)?;
//!     Ok(())
//! }
//! ```
//!
//! # Comparison with `thiserror`
//!
//! `goof::Error` follows the same attribute API as `thiserror`, so
//! migrating between the two is a one-line change in your `use`
//! declaration.  The key differences are:
//!
//! * `goof` generates `::core::error::Error` rather than
//!   `::std::error::Error`, making the output compatible with
//!   `#![no_std]` environments (on compilers that stabilise
//!   `core::error::Error`).
//! * `goof` bundles the reusable error types listed above, so small
//!   projects may not need to define custom error types at all.

use core::fmt::{Debug, Display};
use std::sync::Arc;

pub use goof_derive::Error;

/// Assert that the object is exactly equal to the provided test value.
///
/// # Motivation
///
/// Oftentimes one really only needs an assertion to be propagated
/// upwards.  Given that try blocks are not stable, this syntax has
/// some merit.  This assert can be used inside function arguments, at
/// the tops of functions to get rid of an ugly `if` and makes it
/// explicit that what you want is to do what the standard library's
/// `assert_eq!` does, but to create an error rather than panic.
///
/// # Examples
/// ```rust
/// use goof::{Mismatch, assert_eq};
///
/// fn fallible_func(thing: &[u8]) -> Result<(), Mismatch<usize>> {
///     assert_eq(&32, &thing.len())?;
///
///     Ok(())
/// }
///
/// assert_eq!(fallible_func(&[]).unwrap_err(), assert_eq(&32, &0).unwrap_err())
/// ```
pub fn assert_eq<T: Copy + Eq + Debug + Display>(
    actual: &T,
    expected: &T,
) -> Result<T, Mismatch<T>> {
    if expected.eq(actual) {
        Ok(*expected)
    } else {
        Err(Mismatch {
            expected: *expected,
            actual: *actual,
        })
    }
}

/// Assert that the object is exactly within the boundaries given by
/// the `range` operand.
///
/// # Motivation
///
/// Oftentimes one really only needs an assertion to be propagated
/// upwards.  Given that try blocks are not stable, this syntax has
/// some merit.  This assert can be used inside function arguments, at
/// the tops of functions to get rid of an ugly `if` and makes it
/// explicit that what you want is to do what the standard library's
/// `assert_eq!` does, but to create an error rather than panic.
///
/// # Examples
/// ```rust
/// use goof::{Outside, assert_in};
///
/// fn fallible_func(thing: &[u8]) -> Result<(), Outside<usize>> {
///     assert_in(&thing.len(), &(32..64))?;
///
///     Ok(())
/// }
///
/// assert_eq!(fallible_func(&vec![0; 32]).unwrap_err(), assert_in(&32, &(32..64)).unwrap_err())
/// ```
pub fn assert_in<T: Ord + Copy + Debug + Display>(
    value: &T,
    range: &core::ops::Range<T>,
) -> Result<T, Outside<T>> {
    if value > &range.start && value <= &range.end {
        Ok(*value)
    } else {
        // TODO: isn't Range<T> supposed to be Copy?
        Err(Outside {
            range: range.clone(),
            value: *value,
        })
    }
}

/// This structure should be used in cases where a value must be
/// exactly equal to another value for the process to be valid.
#[derive(PartialEq, Eq, Clone, Copy, Error)]
pub struct Mismatch<T: Copy + Eq + Debug + Display> {
    /// The expected return type
    pub(crate) expected: T,
    /// What was actually received
    pub(crate) actual: T,
}

impl<T: Debug + Copy + Eq + Debug + Display> Debug for Mismatch<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mismatch")
            .field("expected", &self.expected)
            .field("actual", &self.actual)
            .finish()
    }
}

impl<T: Display + Copy + Eq + Debug + Display> Display for Mismatch<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expected {}, but got {}", self.expected, self.actual)
    }
}

/// This structure should be used in cases where a value must lie
/// within a specific range
#[derive(Clone, Error)]
pub struct Outside<T: Ord + Copy + Debug + Display> {
    /// The inclusive range into which the value must enter.
    pub(crate) range: core::ops::Range<T>,
    /// The value that failed to be included into the range.
    pub(crate) value: T,
}

impl<T: Ord + Copy + Debug + Display> Debug for Outside<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outside")
            .field("range", &self.range)
            .field("value", &self.value)
            .finish()
    }
}

impl<T: Ord + Copy + Debug + Display> Display for Outside<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.value >= self.range.end {
            write!(f, "Value {} exceeds maximum {}", self.value, self.range.end)
        } else if self.value < self.range.start {
            write!(f, "Value {} below minimum {}", self.value, self.range.start)
        } else {
            panic!("An invalid instance of outside was created. Aborting")
        }
    }
}

impl<T: PartialEq + Ord + Copy + Debug + Display> PartialEq for Outside<T> {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range && self.value == other.value
    }
}

/// A thing is not a known value from a list
#[derive(PartialEq, Eq, Clone)]
pub struct Unknown<'a, T: Eq> {
    /// The collection of things arranged in a linear sequence
    pub(crate) knowns: Option<&'a [T]>,
    /// The value that is not in the list
    pub(crate) value: T,
}

impl<'a, T: Eq + Copy> Copy for Unknown<'a, T> {}

impl<T: Eq + Debug> Debug for Unknown<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Unknown")
            .field("knowns", &self.knowns)
            .field("value", &self.value)
            .finish()
    }
}

impl<T: Eq + Display> Display for Unknown<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "The value {} is not known", self.value)?;
        if let Some(knowns) = self.knowns {
            write!(f, "Because it's not one of [{}]", join(knowns, ", ")?)
        } else {
            f.write_str(".")
        }
    }
}

pub fn join<T: Display>(items: &[T], separator: &'static str) -> Result<String, core::fmt::Error> {
    use core::fmt::Write;

    let first_element = items[0].to_string();
    let mut buffer = String::with_capacity(
        (items.len() - 1) * (separator.len() + first_element.len()) + first_element.len(),
    );
    for item in items.iter().skip(1) {
        buffer.push_str(separator);
        buffer.write_str(&item.to_string())?;
    }
    Ok(buffer)
}

pub fn assert_known_enum<T: Eq>(knowns: &'_ [T], value: T) -> Result<T, Unknown<'_, T>> {
    if knowns.contains(&value) {
        Ok(value)
    } else {
        Err(Unknown {
            knowns: Some(knowns),
            value,
        })
    }
}

pub fn assert_known<T: Eq>(knowns: &[T], value: T) -> Result<T, Unknown<'_, T>> {
    if knowns.contains(&value) {
        Ok(value)
    } else {
        Err(Unknown {
            knowns: None,
            value,
        })
    }
}

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
pub struct Mishap {
    wrapped_err: Arc<dyn std::error::Error>,
}

impl Display for Mishap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.wrapped_err)
    }
}

impl Debug for Mishap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mishap")
            .field("wrapped_err", &self.wrapped_err)
            .finish()
    }
}

impl core::error::Error for Mishap {}

#[cfg(test)]
pub mod tests {
    use crate::{Mismatch, Outside, Unknown};

    #[test]
    fn usage_of_assert_eq() {
        assert_eq!(crate::assert_eq(&32_u32, &32), Ok(32));
        assert_eq!(
            crate::assert_eq(&32_u32, &33),
            Err(Mismatch {
                expected: 33,
                actual: 32
            })
        );
    }

    #[test]
    fn usage_of_outside() {
        assert_eq!(crate::assert_in(&2, &(1..5)), Ok(2));
        assert_eq!(crate::assert_in(&5, &(1..5)), Ok(5));
        assert_eq!(
            crate::assert_in(&6, &(1..5)),
            Err(Outside {
                range: 1..5,
                value: 6
            })
        );
        assert_eq!(
            crate::assert_in(&0, &(1..5)),
            Err(Outside {
                range: 1..5,
                value: 0
            })
        );
    }

    #[test]
    fn usage_of_unknown() {
        let knowns = vec![1, 2, 4, 6, 7, 20_u32];
        assert_eq!(crate::assert_known_enum(&knowns, 2), Ok(2));
        assert_eq!(
            crate::assert_known_enum(&knowns, 3),
            Err(Unknown {
                knowns: Some(&knowns),
                value: 3
            })
        );
        assert_eq!(crate::assert_known(&knowns, 2), Ok(2));
        assert_eq!(
            crate::assert_known(&knowns, 3),
            Err(Unknown {
                knowns: None,
                value: 3
            })
        );
    }

    // -----------------------------------------------------------------------
    // derive(Error) tests — thiserror-compatible API
    // -----------------------------------------------------------------------
    use crate::Error;

    // --- Simple struct with #[error("...")] ---
    #[derive(Debug, Error)]
    #[error("simple error happened")]
    struct SimpleError;

    #[test]
    fn simple_unit_struct_display() {
        let e = SimpleError;
        assert_eq!(e.to_string(), "simple error happened");
        // source should be None
        assert!(std::error::Error::source(&e).is_none());
    }

    // --- Struct with named field interpolation ---
    #[derive(Debug, Error)]
    #[error("invalid header (expected {expected:?}, found {found:?})")]
    struct InvalidHeader {
        expected: String,
        found: String,
    }

    #[test]
    fn struct_named_field_interpolation() {
        let e = InvalidHeader {
            expected: "foo".into(),
            found: "bar".into(),
        };
        assert_eq!(
            e.to_string(),
            r#"invalid header (expected "foo", found "bar")"#
        );
    }

    // --- Struct with #[source] ---
    #[derive(Debug, Error)]
    #[error("wrapper error")]
    struct WrapperError {
        #[source]
        inner: std::io::Error,
    }

    #[test]
    fn struct_source_attribute() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let e = WrapperError { inner };
        assert!(std::error::Error::source(&e).is_some());
    }

    // --- Struct with field named `source` (auto-detected) ---
    #[derive(Debug, Error)]
    #[error("auto source")]
    struct AutoSourceError {
        source: std::io::Error,
    }

    #[test]
    fn struct_auto_source_field() {
        // exhibit A: A moronic error that you might encounter in the wild
        let source = std::io::Error::other("oops");
        let e = AutoSourceError { source };
        assert!(std::error::Error::source(&e).is_some());
    }

    // --- Struct with #[from] ---
    #[derive(Debug, Error)]
    #[error("from io error")]
    struct FromIoError {
        #[from]
        source: std::io::Error,
    }

    #[test]
    fn struct_from_impl() {
        let io_err = std::io::Error::other("disk full");
        let e: FromIoError = io_err.into();
        assert_eq!(e.to_string(), "from io error");
        assert!(std::error::Error::source(&e).is_some());
    }

    // --- Struct with #[error(transparent)] ---
    #[derive(Debug, Error)]
    #[error(transparent)]
    struct TransparentError(std::io::Error);

    #[test]
    fn struct_transparent() {
        let inner = std::io::Error::new(std::io::ErrorKind::Other, "inner msg");
        let e = TransparentError(inner);
        // Display delegates to inner
        assert_eq!(e.to_string(), "inner msg");
        // source delegates to inner
        assert!(std::error::Error::source(&e).is_some());
    }

    // --- Enum with variants ---
    #[derive(Debug, Error)]
    enum DataStoreError {
        #[error("data store disconnected")]
        Disconnect(#[from] std::io::Error),
        #[error("the data for key `{0}` is not available")]
        Redaction(String),
        #[error("invalid header (expected {expected:?}, found {found:?})")]
        InvalidHeader { expected: String, found: String },
        #[error("unknown data store error")]
        Unknown,
    }

    #[test]
    fn enum_display_unit_variant() {
        let e = DataStoreError::Unknown;
        assert_eq!(e.to_string(), "unknown data store error");
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn enum_display_tuple_variant() {
        let e = DataStoreError::Redaction("my_key".into());
        assert_eq!(e.to_string(), "the data for key `my_key` is not available");
    }

    #[test]
    fn enum_display_named_variant() {
        let e = DataStoreError::InvalidHeader {
            expected: "json".into(),
            found: "xml".into(),
        };
        assert_eq!(
            e.to_string(),
            r#"invalid header (expected "json", found "xml")"#
        );
    }

    #[test]
    fn enum_from_variant() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
        let e: DataStoreError = io_err.into();
        assert_eq!(e.to_string(), "data store disconnected");
        assert!(std::error::Error::source(&e).is_some());
    }

    // --- Enum with #[error(transparent)] variant ---
    #[derive(Debug, Error)]
    enum MixedError {
        #[error("known error")]
        Known,
        #[error(transparent)]
        Other(std::io::Error),
    }

    #[test]
    fn enum_transparent_variant() {
        let inner = std::io::Error::new(std::io::ErrorKind::Other, "opaque");
        let e = MixedError::Other(inner);
        assert_eq!(e.to_string(), "opaque");
        assert!(std::error::Error::source(&e).is_some());
    }

    // --- No #[error] attribute: Display must be provided manually ---
    #[derive(Debug, Error)]
    struct ManualDisplay {
        msg: String,
    }

    impl std::fmt::Display for ManualDisplay {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "manual: {}", self.msg)
        }
    }

    #[test]
    fn struct_manual_display() {
        let e = ManualDisplay {
            msg: "hello".into(),
        };
        assert_eq!(e.to_string(), "manual: hello");
    }
}
