//! A value is not among a known set: [`Unknown`] and the
//! [`assert_known`] / [`assert_known_enum`] helpers.

use core::fmt::{Debug, Display};

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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Unknown")
            .field("knowns", &self.knowns)
            .field("value", &self.value)
            .finish()
    }
}

impl<T: Eq + Display> Display for Unknown<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "The value {} is not known", self.value)?;
        if let Some(knowns) = self.knowns {
            // Write the known values straight into the formatter, so the
            // whole impl stays allocation-free (and therefore `no_std`).
            f.write_str(" because it's not one of [")?;
            for (i, item) in knowns.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{item}")?;
            }
            f.write_str("].")
        } else {
            f.write_str(".")
        }
    }
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

#[cfg(test)]
mod tests {
    use crate::Unknown;

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
}
