//! The goof library is a collection of re-usable error handling
//! structs and patterns that are meant to make error handling
//! lightweight, portable and inter-convertible.

use core::fmt::{Debug, Display};

pub fn assert_eq<T: Copy + Eq>(expected: &T, actual: &T) -> Result<T, Mismatch<T>> {
    if expected.eq(&actual) {
        Ok(*expected)
    } else {
        Err(Mismatch { expected: *expected, actual: *actual})
    }
}


pub fn assert_in<T: Ord + Copy>(value: &T, range: &core::ops::Range<T>) -> Result<T, Outside<T>>{
    if value > &range.start && value <= &range.end {
        Ok(*value)
    } else {
        // TODO: isn't Range<T> supposed to be Copy?
        Err(Outside{ range: range.clone(), value: *value})
    }
}


/// A value was one thing, instead of another thing.
#[derive(PartialEq, Eq)]
pub struct Mismatch<T: Copy + Eq> {
    /// The expected return type 
    pub(crate) expected: T,
    /// What was actually received
    pub(crate) actual: T
}

impl<T: Debug + Copy + Eq> Debug for Mismatch<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mismatch").field("expected", &self.expected).field("actual", &self.actual).finish()
    }
}

impl<T: Display + Copy + Eq> Display for Mismatch<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expected {}, but got {}", self.expected, self.actual)
    }
}



/// A Value fell outside of an acceptable range
pub struct Outside<T: Ord + Copy> {
    /// The inclusive range into which the value must enter.
    pub(crate) range: core::ops::Range<T>,
    /// The value that failed to be included into the range.
    pub(crate) value: T
}

impl<T: Ord+ Copy + Debug> Debug for Outside<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outside").field("range", &self.range).field("value", &self.value).finish()
    }
}

impl<T: Ord+ Copy+ Display> Display for Outside<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.value > self.range.end {
            write!(f, "Value {} exceeds maximum {}", self.value, self.range.end)
        } else if self.value < self.range.start {
            write!(f, "Value {} below minimum {}", self.value, self.range.start)
        } else {
            panic!("An invalid instance of outside was created. Aborting")
        }
    }
}

impl<T: PartialEq + Ord + Copy> PartialEq for Outside<T> {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range && self.value == other.value
    }
}


/// A thing is not a known value from a list
#[derive(PartialEq, Eq)]
pub struct Unknown<'a, T: Eq> {
    /// The collection of things arranged in a linear sequence
    pub(crate) knowns: Option<&'a [T]>,
    /// The value that is not in the list
    pub(crate) value: T,
}

impl<T: Eq + Debug> Debug for Unknown<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Unknown").field("knowns", &self.knowns).field("value", &self.value).finish()
    }
}

impl<T: Eq + Display> Display for Unknown<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "The value {} is not known", self.value )?;
        if let Some(knowns) = self.knowns {
            write!(f, "Because it's not one of [{}]", join(&knowns, ", ")?)
        } else {
            f.write_str(".")
        }
    }
}

pub fn join<T: Display>(items: &[T], separator: &'static str) -> Result<String, core::fmt::Error> {
    use core::fmt::Write;
    
    let first_element = items[0].to_string();
    let mut buffer = String::with_capacity((items.len()-1)*(separator.len() + first_element.len())+ first_element.len());
    for idx in 1..items.len() {
        buffer.push_str(separator);
        buffer.write_str(&items[idx].to_string())?;
    }
    Ok(buffer)
}


pub fn assert_known_enum<'a, T: Eq>(knowns: &'a [T], value: T) -> Result<T, Unknown<'a, T>>{
    if knowns.contains(&value) {
        Ok(value)
    } else {
        Err(Unknown {
            knowns: Some(knowns),
            value
        })
    }
}

pub fn assert_known<'a, T: Eq>(knowns: &'a [T], value: T)-> Result<T, Unknown<'_, T>> {
    if knowns.contains(&value) {
        Ok(value)
    } else {
        Err(Unknown {
            knowns: None,
            value
        })
    }
}


#[cfg(test)]
pub mod tests {
    use crate::{Outside, Mismatch, Unknown};

    #[test]
    fn usage_of_assert_eq() {
        assert_eq!(crate::assert_eq(&32_u32, &32), Ok(32));
        assert_eq!(crate::assert_eq(&32_u32, &33), Err(Mismatch { expected: 32, actual: 33 }));
    }

    #[test]
    fn usage_of_outside() {
        assert_eq!(crate::assert_in(&2, &(1..5)), Ok(2));
        assert_eq!(crate::assert_in(&5, &(1..5)), Ok(5));
        assert_eq!(crate::assert_in(&6, &(1..5)), Err(Outside{ range: 1..5, value: 6}));
        assert_eq!(crate::assert_in(&0, &(1..5)), Err(Outside{ range: 1..5, value: 0}));
    }

    #[test]
    fn usage_of_unknown() {
        let knowns = vec![1, 2, 4, 6, 7, 20_u32];
        assert_eq!(crate::assert_known_enum(&knowns, 2),Ok(2) );
        assert_eq!(crate::assert_known_enum(&knowns, 3),Err(Unknown{knowns: Some(&knowns), value: 3}) );
        assert_eq!(crate::assert_known(&knowns, 2),Ok(2) );
        assert_eq!(crate::assert_known(&knowns, 3),Err(Unknown{knowns: None, value: 3}) );
    }
}
