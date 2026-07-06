//! Accumulate several errors instead of propagating the first one:
//! [`Errors`].

#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::fmt::{Debug, Display};

/// A fixed-capacity, stack-allocated collection of errors gathered
/// while "failing completely".
///
/// Where the `assert_*` helpers and the `?` operator "fail fast" —
/// bailing out on the first problem — [`Errors`] lets you keep going,
/// stashing each failure so the caller sees *all* of them at once.  This
/// is the shape you want when validating a form, parsing a config file,
/// or checking a batch: one pass, every complaint reported together.
///
/// The first `N` errors live inline, on the stack, so the type is
/// usable in the allocator-free core and is [`Copy`] whenever `E` is.
/// `N` defaults to `10`; if you genuinely need more while staying
/// `no_std`, raise it explicitly.
///
/// Overflow past `N` depends on the environment:
///
/// - with the `alloc` feature, extra errors spill onto the heap and the
///   collection is effectively unbounded;
/// - without it, extra errors are dropped and counted (see
///   [`dropped`](Self::dropped)), which surfaces in the [`Display`]
///   output rather than silently vanishing.
///
/// ```rust
/// use goof::{Errors, Mismatch, assert_eq};
///
/// let mut errors: Errors<Mismatch<i32>> = Errors::new();
/// for (actual, expected) in [(1, 1), (2, 3), (4, 5)] {
///     if let Err(e) = assert_eq(&actual, &expected) {
///         errors.push(e);
///     }
/// }
///
/// // Two of the three checks failed.
/// assert_eq!(errors.len(), 2);
/// let result: Result<(), Errors<Mismatch<i32>>> = errors.into_result();
/// assert_eq!(result.unwrap_err().len(), 2);
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Errors<E, const N: usize = 10> {
    /// Inline storage; slots `[0..len)` are always `Some`.
    items: [Option<E>; N],
    /// Number of filled inline slots.
    len: usize,
    /// Errors that overflowed the inline buffer, spilled onto the heap.
    #[cfg(feature = "alloc")]
    overflow: Vec<E>,
    /// Errors dropped because the inline buffer was full and no
    /// allocator was available to spill onto.
    #[cfg(not(feature = "alloc"))]
    dropped: usize,
}

impl<E, const N: usize> Errors<E, N> {
    /// Create an empty collection.
    pub const fn new() -> Self {
        Self {
            items: [const { None }; N],
            len: 0,
            #[cfg(feature = "alloc")]
            overflow: Vec::new(),
            #[cfg(not(feature = "alloc"))]
            dropped: 0,
        }
    }

    /// Add an error to the collection.
    ///
    /// Stored inline while there is room; otherwise spilled onto the
    /// heap (with `alloc`) or dropped and counted (without it).
    pub fn push(&mut self, error: E) {
        if self.len < N {
            self.items[self.len] = Some(error);
            self.len += 1;
        } else {
            #[cfg(feature = "alloc")]
            self.overflow.push(error);
            #[cfg(not(feature = "alloc"))]
            {
                // No allocator to grow into: drop the error, but keep a
                // tally so the loss is visible rather than silent.
                let _ = error;
                self.dropped += 1;
            }
        }
    }

    /// The number of errors currently held (inline plus any spilled).
    ///
    /// This does **not** include errors [`dropped`](Self::dropped) for
    /// lack of an allocator.
    pub fn len(&self) -> usize {
        #[cfg(feature = "alloc")]
        {
            self.len + self.overflow.len()
        }
        #[cfg(not(feature = "alloc"))]
        {
            self.len
        }
    }

    /// The number of errors discarded because the inline buffer filled
    /// up without an allocator to spill onto.  Always `0` when the
    /// `alloc` feature is enabled.
    pub fn dropped(&self) -> usize {
        #[cfg(feature = "alloc")]
        {
            0
        }
        #[cfg(not(feature = "alloc"))]
        {
            self.dropped
        }
    }

    /// Whether nothing was recorded — no held errors and none dropped.
    pub fn is_empty(&self) -> bool {
        self.len() == 0 && self.dropped() == 0
    }

    /// Iterate over the held errors in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &E> {
        let inline = self.items.iter().flatten();
        #[cfg(feature = "alloc")]
        {
            inline.chain(self.overflow.iter())
        }
        #[cfg(not(feature = "alloc"))]
        {
            inline
        }
    }

    /// Collapse into a [`Result`]: `Ok(())` when empty, otherwise
    /// `Err(self)`.  Use this at the end of a "fail completely" pass to
    /// hand every accumulated error to the caller in one go.
    pub fn into_result(self) -> Result<(), Self> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl<E, const N: usize> Default for Errors<E, N> {
    fn default() -> Self {
        Self::new()
    }
}

// Best-effort `Copy`, per the crate's "structs that could be Copy, are"
// goal.  Only possible without the heap-backed overflow buffer.
#[cfg(not(feature = "alloc"))]
impl<E: Copy, const N: usize> Copy for Errors<E, N> {}

impl<E, const N: usize> Extend<E> for Errors<E, N> {
    fn extend<I: IntoIterator<Item = E>>(&mut self, iter: I) {
        for error in iter {
            self.push(error);
        }
    }
}

impl<E, const N: usize> FromIterator<E> for Errors<E, N> {
    fn from_iter<I: IntoIterator<Item = E>>(iter: I) -> Self {
        let mut errors = Self::new();
        errors.extend(iter);
        errors
    }
}

/// Owning iterator over the errors held by [`Errors`], in insertion
/// order.  Created by [`Errors::into_iter`](IntoIterator::into_iter).
pub struct ErrorsIntoIter<E, const N: usize> {
    inline: core::iter::Flatten<core::array::IntoIter<Option<E>, N>>,
    #[cfg(feature = "alloc")]
    overflow: alloc::vec::IntoIter<E>,
}

impl<E, const N: usize> Iterator for ErrorsIntoIter<E, N> {
    type Item = E;

    fn next(&mut self) -> Option<E> {
        if let Some(error) = self.inline.next() {
            return Some(error);
        }
        #[cfg(feature = "alloc")]
        {
            self.overflow.next()
        }
        #[cfg(not(feature = "alloc"))]
        {
            None
        }
    }
}

impl<E, const N: usize> IntoIterator for Errors<E, N> {
    type Item = E;
    type IntoIter = ErrorsIntoIter<E, N>;

    fn into_iter(self) -> Self::IntoIter {
        ErrorsIntoIter {
            inline: self.items.into_iter().flatten(),
            #[cfg(feature = "alloc")]
            overflow: self.overflow.into_iter(),
        }
    }
}

impl<E: Debug, const N: usize> Debug for Errors<E, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<E: Display, const N: usize> Display for Errors<E, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} error(s) occurred:", self.len())?;
        for error in self.iter() {
            write!(f, "\n  - {error}")?;
        }
        if self.dropped() > 0 {
            write!(f, "\n  ... and {} more (buffer full)", self.dropped())?;
        }
        Ok(())
    }
}

impl<E: core::error::Error, const N: usize> core::error::Error for Errors<E, N> {}

#[cfg(test)]
mod tests {
    use crate::{assert_eq as goof_eq, Errors, Mismatch};

    #[test]
    fn empty_is_ok() {
        let errors: Errors<Mismatch<i32>> = Errors::new();
        assert!(errors.is_empty());
        assert_eq!(errors.len(), 0);
        assert!(errors.into_result().is_ok());
    }

    #[test]
    fn accumulates_and_reports_all() {
        let mut errors: Errors<Mismatch<i32>> = Errors::new();
        for (actual, expected) in [(1, 1), (2, 3), (4, 5)] {
            if let Err(e) = goof_eq(&actual, &expected) {
                errors.push(e);
            }
        }
        assert_eq!(errors.len(), 2);
        assert_eq!(errors.iter().count(), 2);
        let err = errors.into_result().unwrap_err();
        assert_eq!(err.len(), 2);
    }

    #[test]
    fn collects_from_iterator() {
        let errors: Errors<Mismatch<i32>> = [(2, 3), (4, 5)]
            .into_iter()
            .filter_map(|(a, b)| goof_eq(&a, &b).err())
            .collect();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn extend_and_owned_iteration() {
        let mut errors: Errors<Mismatch<i32>> = Errors::new();
        errors.extend([(2, 3), (4, 5)].into_iter().filter_map(|(a, b)| goof_eq(&a, &b).err()));
        assert_eq!(errors.len(), 2);

        let actuals: [i32; 2] = {
            let mut it = errors.into_iter().map(|m| m.actual);
            [it.next().unwrap(), it.next().unwrap()]
        };
        assert_eq!(actuals, [2, 4]);
    }

    #[test]
    fn small_capacity_keeps_insertion_order() {
        let mut errors: Errors<Mismatch<i32>, 2> = Errors::new();
        for (actual, expected) in [(1, 2), (3, 4), (5, 6)] {
            if let Err(e) = goof_eq(&actual, &expected) {
                errors.push(e);
            }
        }
        // With `alloc`, the third error spills to the heap; without it,
        // it is dropped and counted. Either way the first two are kept
        // in order.
        let mut it = errors.iter();
        assert_eq!(it.next().unwrap().actual, 1);
        assert_eq!(it.next().unwrap().actual, 3);
    }
}
