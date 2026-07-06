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
//!     #[error("Failed to set mpv option {option}: {detail}")]
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

pub use goof_derive::Error;

mod context;
mod mishap;
mod mismatch;
mod outside;
mod unknown;

pub use context::Context;
pub use mishap::Mishap;
pub use mismatch::{assert_eq, Mismatch};
pub use outside::{assert_in, Outside};
pub use unknown::{assert_known, assert_known_enum, join, Unknown};
