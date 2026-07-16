//! A value is not among a known set: [`Unknown`] and the
//! [`assert_known`] / [`assert_known_enum`] helpers.

use core::fmt::{Debug, Display};

/// A value is not among a known set.
///
/// The offending witness `V` and the known-set element `K` are separate types
/// (default `K = V`), so the witness may be owned while the set is borrowed —
/// e.g. a parser reports an owned `String` against a `&'static [&'static str]`
/// without tying the error's lifetime to its input. When they coincide (the
/// common case) the default keeps `Unknown<'a, T>` reading as before.
#[derive(PartialEq, Eq, Clone)]
pub struct Unknown<'a, V, K = V> {
    /// The collection of things arranged in a linear sequence
    pub(crate) knowns: Option<&'a [K]>,
    /// The value that is not in the list
    pub(crate) value: V,
}

impl<V: Copy, K: Copy> Copy for Unknown<'_, V, K> {}

impl<V: Debug, K: Debug> Debug for Unknown<'_, V, K> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Unknown")
            .field("knowns", &self.knowns)
            .field("value", &self.value)
            .finish()
    }
}

impl<V: Display, K: Display> Display for Unknown<'_, V, K> {
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

impl<V: Debug + Display, K: Debug + Display> core::error::Error for Unknown<'_, V, K> {}

pub fn assert_known_enum<'a, V, K>(
    knowns: &'a [K],
    value: V,
) -> Result<V, Unknown<'a, V, K>>
where
    V: PartialEq<K>,
    K: Copy,
{
    if knowns.iter().any(|known| value == *known) {
        Ok(value)
    } else {
        Err(Unknown {
            knowns: Some(knowns),
            value,
        })
    }
}

pub fn assert_known<'a, V, K>(
    knowns: &'a [K],
    value: V,
) -> Result<V, Unknown<'a, V, K>>
where
    V: PartialEq<K>,
    K: Copy,
{
    if knowns.iter().any(|known| value == *known) {
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
        let knowns = [1, 2, 4, 6, 7, 20_u32];
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

    #[test]
    fn witness_and_set_may_differ() {
        // Owned witness (`String`) against a borrowed static set (`&str`): the
        // shape a `FromStr::Err` needs, which the single-`T` form could not
        // express.
        let knowns: &[&str] = &["left", "right"];
        assert_eq!(
            crate::assert_known_enum(knowns, "left".to_owned()),
            Ok("left".to_owned())
        );
        let err = crate::assert_known_enum(knowns, "up".to_owned()).unwrap_err();
        assert_eq!(err.value, "up");
        assert_eq!(
            err.to_string(),
            "The value up is not known because it's not one of [left, right]."
        );
    }
}
