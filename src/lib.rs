//! The goof library is a collection of re-usable error handling
//! structs and patterns that are meant to make error handling
//! lightweight, portable and inter-convertible.

/// A value was one thing, instead of another thing.
pub struct Mismatch<T: Copy + Eq> {
    expected: T,
    actual: T
}


