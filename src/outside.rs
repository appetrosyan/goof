//! A value fell outside a required range: [`Outside`] and [`assert_in`].

use core::fmt::{Debug, Display};
use core::ops::{Bound, RangeBounds};

use crate::Error;

/// Assert that `value` lies within `range`.
///
/// Accepts any [`RangeBounds`], so every range syntax works: `a..b`, `a..=b`,
/// `a..`, `..b`, `..=b` and `..`.
///
/// # Motivation
///
/// Oftentimes one really only needs an assertion to be propagated upwards.
/// Given that try blocks are not stable, this syntax has some merit.  This
/// assert can be used inside function arguments, at the tops of functions to
/// get rid of an ugly `if`, and makes it explicit that what you want is to do
/// what the standard library's `assert!` does, but to create an error rather
/// than panic.
///
/// # Examples
/// ```rust
/// use goof::{Outside, assert_in};
///
/// fn check_len(thing: &[u8]) -> Result<(), Outside<usize>> {
///     assert_in(&thing.len(), 32..64)?;
///     Ok(())
/// }
///
/// // 16 is below the minimum of 32.
/// assert!(check_len(&[0; 16]).is_err());
/// // 40 lies within `32..64`.
/// assert!(check_len(&[0; 40]).is_ok());
/// ```
pub fn assert_in<T, R>(value: &T, range: R) -> Result<T, Outside<T>>
where
    T: Ord + Copy + Debug + Display,
    R: RangeBounds<T>,
{
    check(
        *value,
        copied(range.start_bound()),
        copied(range.end_bound()),
    )
}

/// Copy a borrowed bound into an owned one.
fn copied<T: Copy>(bound: Bound<&T>) -> Bound<T> {
    match bound {
        Bound::Unbounded => Bound::Unbounded,
        Bound::Included(v) => Bound::Included(*v),
        Bound::Excluded(v) => Bound::Excluded(*v),
    }
}

/// The range-agnostic core: monomorphises only over `T`, never over the range
/// type the public [`assert_in`] shim was called with.
fn check<T: Ord + Copy + Debug + Display>(
    value: T,
    start: Bound<T>,
    end: Bound<T>,
) -> Result<T, Outside<T>> {
    let above_start = match start {
        Bound::Unbounded => true,
        Bound::Included(s) => value >= s,
        Bound::Excluded(s) => value > s,
    };
    let below_end = match end {
        Bound::Unbounded => true,
        Bound::Included(e) => value <= e,
        Bound::Excluded(e) => value < e,
    };
    if above_start && below_end {
        Ok(value)
    } else {
        Err(Outside { start, end, value })
    }
}

/// This structure should be used in cases where a value must lie within a
/// specific range.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub struct Outside<T: Ord + Copy + Debug + Display> {
    /// The lower end of the required range.
    pub(crate) start: Bound<T>,
    /// The upper end of the required range.
    pub(crate) end: Bound<T>,
    /// The value that failed to enter the range.
    pub(crate) value: T,
}

impl<T: Ord + Copy + Debug + Display> Display for Outside<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.start {
            Bound::Included(s) if self.value < s => {
                return write!(f, "Value {} is below the minimum {}", self.value, s);
            }
            Bound::Excluded(s) if self.value <= s => {
                return write!(f, "Value {} must be greater than {}", self.value, s);
            }
            _ => {}
        }
        match self.end {
            Bound::Included(e) if self.value > e => {
                write!(f, "Value {} exceeds the maximum {}", self.value, e)
            }
            Bound::Excluded(e) if self.value >= e => {
                write!(f, "Value {} must be less than {}", self.value, e)
            }
            _ => write!(f, "Value {} is out of range", self.value),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{assert_in, Outside};
    use core::ops::Bound;

    #[test]
    fn half_open_range() {
        assert_eq!(assert_in(&2, 1..5), Ok(2));
        assert_eq!(assert_in(&4, 1..5), Ok(4));
        // The end is exclusive.
        assert!(assert_in(&5, 1..5).is_err());
        assert!(assert_in(&6, 1..5).is_err());
        assert!(assert_in(&0, 1..5).is_err());
        assert_eq!(
            assert_in(&6, 1..5),
            Err(Outside {
                start: Bound::Included(1),
                end: Bound::Excluded(5),
                value: 6,
            })
        );
    }

    #[test]
    fn other_range_kinds() {
        assert_eq!(assert_in(&5, 1..=5), Ok(5));
        assert_eq!(assert_in(&100, 1..), Ok(100));
        assert!(assert_in(&5, ..5).is_err());
        assert_eq!(assert_in(&4, ..5), Ok(4));
        assert_eq!(assert_in(&i32::MIN, ..), Ok(i32::MIN));
    }
}
