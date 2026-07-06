//! A value fell outside a required range: [`Outside`] and [`assert_in`].

use core::fmt::{Debug, Display};

use crate::Error;

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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Outside")
            .field("range", &self.range)
            .field("value", &self.value)
            .finish()
    }
}

impl<T: Ord + Copy + Debug + Display> Display for Outside<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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

#[cfg(test)]
mod tests {
    use crate::Outside;

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
}
