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
