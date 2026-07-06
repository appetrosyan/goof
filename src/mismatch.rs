//! An equality check failed: [`Mismatch`] and its [`assert_eq`] helper.

use core::fmt::{Debug, Display};

use crate::Error;

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

/// This structure should be used in cases where a value must be
/// exactly equal to another value for the process to be valid.
#[derive(PartialEq, Eq, Clone, Copy, Error)]
pub struct Mismatch<T: Copy + Eq + Debug + Display> {
    /// The expected return type
    pub expected: T,
    /// What was actually received
    pub actual: T,
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

#[cfg(test)]
mod tests {
    use crate::Mismatch;

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
}
