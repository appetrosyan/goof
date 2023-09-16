/// The simplest type of error that can be created. This is
/// essentially a wrapper around `str` with the intention of
/// simplifying the process of defaulting to `String` but without
/// actually doing anything stupid with it.
#[derive(Debug)]
pub struct Goof<'a> {
    // TODO: add a const generic parameter such that the size of the
    // string slice can be known at compile time, and goofs could be
    // built up from Strings without cloning. This is similar to
    // Pascal strings, with one big difference, the strings can be
    // resized once the goof had been finalised.
    message: &'a str,
}

impl<'a> core::fmt::Display for Goof<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message.trim())
    }
}

// impl<'a, T: PartialOrd> Mismatch<'a, T> {
//     fn expecting(thing: &'a T) -> Self {
//         todo!()
//     }
// }

// pub struct Mismatch<'a, T: PartialEq> {
//     /// The thing that we expect.
//     expected: &'a T,

//     /// The thing that we got.
//     actual: &'a T,
// }

pub struct RangeError<T: PartialOrd + Copy> {
    start: T,
    end: T,
    actual: T,
}

impl<'a> TryFrom<String> for Goof<'a> {
    type Error = RangeError<usize>;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        // You should not be using more than the placeholder len.
        let mut message_placeholder = "                                        "; // 40 characters whitespace
        for i in [0..core::cmp::min(message_placeholder.len(), value.len())] {
            message_placeholder.get_unchecked_mut(i) = value[i];
        }
        Ok(Goof {
            message: message_placeholder,
        })
    }
}

pub fn goof<'a>(message: &'a str) -> Goof<'a> {
    Goof { message }
}

fn main() {
    println!("{}", goof("Hello world"));
}
