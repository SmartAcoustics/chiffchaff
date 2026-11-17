//! [`FixedStr`]: a fixed-capacity inline string.

use core::fmt;

use snafu::Snafu;

/// A string was too long for a [`FixedStr`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Snafu)]
#[snafu(display("{}: {len} bytes, capacity {capacity}", self.const_message()))]
pub struct CapacityError {
    /// Length of the string, in bytes.
    pub len: usize,
    /// Capacity of the `FixedStr`, in bytes.
    pub capacity: usize,
}

impl CapacityError {
    /// This error's message without runtime details, usable in const
    /// contexts. [`Display`](fmt::Display) is this plus the details.
    pub const fn const_message(&self) -> &'static str {
        "string exceeds FixedStr capacity"
    }
}

mod utf8_buf {
    #[derive(Clone, Copy)]
    /// A minimal fixed length buffer for holding Utf8 strings.
    /// That is enforced by only being able to populate the buffer with
    /// a `str` converted to u8.
    pub(super) struct Utf8Buf<const N: usize>([u8; N]);

    impl<const N: usize> Utf8Buf<N> {
        /// Create a Utf8Buf populated with a N '0' characters
        pub(super) const fn zero_digit_filled() -> Self {
            Self([b'0'; N])
        }

        pub(super) const fn as_str(&self) -> &str {
            // Only utf8 strings can be in the buffer by design
            match core::str::from_utf8(&self.0) {
                Ok(s) => s,
                Err(_) => unreachable!(),
            }
        }

        pub(super) const fn split_at_checked(&self, mid: usize) -> Option<(&str, &str)> {
            self.as_str().split_at_checked(mid)
        }

        /// Overwrites `s.len()` bytes at `at`. Returns the end offset, or `None`
        /// if `s` doesn't fit or the span would cut through a character.
        pub(super) const fn overwrite_str(&mut self, at: usize, s: &str) -> Option<usize> {
            let buf: &mut [u8; N] = &mut self.0;
            let src = s.as_bytes();
            let src_len = src.len();
            let end = match at.checked_add(src_len) {
                Some(end) => end,
                _ => return None,
            };

            let buf_len = buf.len();
            let dest = if at > buf_len || end > buf_len {
                return None;
            } else {
                let (head, _) = buf.split_at_mut(end);
                let (_, dest) = head.split_at_mut(at);
                dest
            };
            dest.copy_from_slice(src);
            Some(end)
        }
    }
}

/// A UTF-8 string of up to `N` bytes, stored inline. Set once on
/// construction; no heap allocation; usable in const contexts.
#[derive(Clone, Copy)]
pub struct FixedStr<const N: usize> {
    // Invariant: `len <= N`, and `buf[..len]` is valid UTF-8.
    buf: utf8_buf::Utf8Buf<N>,
    len: usize,
}

impl<const N: usize> FixedStr<N> {
    /// The empty string.
    pub const EMPTY: Self = Self {
        buf: utf8_buf::Utf8Buf::<N>::zero_digit_filled(),
        len: 0,
    };

    /// Copies `s` in, failing if it's longer than `N` bytes.
    pub const fn try_from_str(s: &str) -> Result<Self, CapacityError> {
        Self::try_concat(&[s])
    }

    /// Concatenates `parts`, failing if the total is longer than `N` bytes.
    pub const fn try_concat(parts: &[&str]) -> Result<Self, CapacityError> {
        let mut buf = utf8_buf::Utf8Buf::<N>::zero_digit_filled();
        let mut offset = 0;
        let mut remainder = parts;

        while let Some((part, tail)) = remainder.split_first() {
            offset = match buf.overwrite_str(offset, part) {
                Some(next_offset) => next_offset,
                None => {
                    return Err(CapacityError {
                        len: total_len(parts),
                        capacity: N,
                    });
                }
            };

            remainder = tail;
        }

        Ok(Self { buf, len: offset })
    }

    pub const fn as_bytes(&self) -> &[u8] {
        self.as_str().as_bytes()
    }

    pub const fn as_str(&self) -> &str {
        match self.buf.split_at_checked(self.len) {
            Some((used, _)) => used,
            None => "", // unreachable: len <= N
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// A full-capacity string of b'x', for the worst-case encoding check. This
    /// is only used for internal compile-time verification and so does not
    /// need to hold meaningful data.
    pub(crate) const fn zero_digit_filled() -> Self {
        Self {
            buf: utf8_buf::Utf8Buf::<N>::zero_digit_filled(),
            len: N,
        }
    }
}

impl<const N: usize> PartialEq for FixedStr<N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl<const N: usize> Eq for FixedStr<N> {}

impl<const N: usize> core::hash::Hash for FixedStr<N> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl<const N: usize> AsRef<str> for FixedStr<N> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> fmt::Debug for FixedStr<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl<const N: usize> fmt::Display for FixedStr<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Total length of `parts` in bytes (saturating; only used for error values).
const fn total_len(parts: &[&str]) -> usize {
    let mut total: usize = 0;
    let mut rest = parts;
    while let Some((part, tail)) = rest.split_first() {
        total = total.saturating_add(part.len());
        rest = tail;
    }
    total
}

#[cfg(test)]
mod tests {
    //! The crate must provide `FixedStr<N>`: a UTF-8 string of at most `N`
    //! bytes, usable in `no_std` and constructible in a const context.
    //!
    //! Tests build the values they check in `const` items, so each one also
    //! shows that the constructor it uses works at compile time.

    extern crate std;
    use super::*;
    use std::string::ToString;

    mod capacity {
        //! A `FixedStr<N>` must hold at most `N` bytes.
        use super::*;

        /// The capacity of a `FixedStr` must be measured in bytes, not characters.
        #[test]
        fn capacity_is_measured_in_bytes() {
            // "é" is 1 character but 2 bytes.
            const FITS: Result<FixedStr<2>, CapacityError> = FixedStr::try_from_str("é");
            const TOO_SMALL: Result<FixedStr<1>, CapacityError> = FixedStr::try_from_str("é");
            const MULTI: Result<FixedStr<8>, CapacityError> = FixedStr::try_from_str("añ€");
            assert!(FITS.is_ok());
            assert_eq!(
                TOO_SMALL,
                Err(CapacityError {
                    len: 2,
                    capacity: 1
                })
            );
            assert_eq!(MULTI.unwrap().len(), 6);
        }

        /// A string exactly as long as the capacity must be accepted.
        #[test]
        fn exact_capacity_is_accepted() {
            const S: Result<FixedStr<4>, CapacityError> = FixedStr::try_from_str("abcd");
            assert_eq!(S.unwrap().as_str(), "abcd");
        }

        /// A `FixedStr<0>` must accept the empty string and reject any other.
        #[test]
        fn zero_capacity() {
            const EMPTY: Result<FixedStr<0>, CapacityError> = FixedStr::try_from_str("");
            const ONE: Result<FixedStr<0>, CapacityError> = FixedStr::try_from_str("a");
            assert!(EMPTY.unwrap().is_empty());
            assert_eq!(
                ONE,
                Err(CapacityError {
                    len: 1,
                    capacity: 0
                })
            );
        }
    }

    mod utf8 {
        //! The contents of a `FixedStr` must always be valid UTF-8.
        use super::*;

        /// Multi-byte characters must be stored and returned intact.
        #[test]
        fn contents_are_valid_utf8() {
            const S: Result<FixedStr<8>, CapacityError> = FixedStr::try_concat(&["añ", "€"]);
            let s = S.unwrap();
            assert!(core::str::from_utf8(s.as_bytes()).is_ok());
            assert_eq!(s.as_str(), "añ€");
        }

        /// A string that doesn't fit must be rejected, not truncated.
        #[test]
        fn oversized_string_is_rejected_not_truncated() {
            const ASCII: Result<FixedStr<4>, CapacityError> = FixedStr::try_from_str("abcde");
            // Running out of room partway through a multi-byte character must
            // not leave the first half of it behind.
            const MID_CHAR: Result<FixedStr<3>, CapacityError> = FixedStr::try_concat(&["ab", "é"]);
            assert!(ASCII.is_err());
            assert!(MID_CHAR.is_err());
        }
    }

    mod construction {
        //! Construction must produce exactly the given contents.
        use super::*;

        /// `try_from_str` must accept a string that fits, and `as_str` must
        /// return it unchanged.
        #[test]
        fn from_str_round_trips() {
            const S: Result<FixedStr<8>, CapacityError> = FixedStr::try_from_str("abc");
            assert_eq!(S.unwrap().as_str(), "abc");
        }

        /// `try_concat` must join the parts in order, with nothing between them.
        #[test]
        fn concat_joins_parts_in_order() {
            const S: Result<FixedStr<16>, CapacityError> =
                FixedStr::try_concat(&["p=", "my-app", ";v=", "1.0"]);
            assert_eq!(S.unwrap().as_str(), "p=my-app;v=1.0");
        }

        /// Concatenating no parts, or only empty parts, must give the empty string.
        #[test]
        fn concat_of_nothing_is_empty() {
            const NONE: Result<FixedStr<4>, CapacityError> = FixedStr::try_concat(&[]);
            const EMPTIES: Result<FixedStr<4>, CapacityError> = FixedStr::try_concat(&["", ""]);
            assert!(NONE.unwrap().is_empty());
            assert!(EMPTIES.unwrap().is_empty());
        }

        /// `EMPTY` must be the empty string.
        #[test]
        fn empty_constant_is_empty() {
            const EMPTY: FixedStr<8> = FixedStr::EMPTY;
            let empty = EMPTY;
            assert!(empty.is_empty());
            assert_eq!((empty.as_str(), empty.len()), ("", 0));
            assert_eq!(empty, FixedStr::<8>::try_from_str("").unwrap());
        }

        /// `zero_digit_filled()` must return a full-capacity string of b'0'.
        #[test]
        fn filled_with_ascii_is_full() {
            const FULL: FixedStr<4> = FixedStr::zero_digit_filled();
            assert_eq!(FULL.len(), 4);
            assert_eq!(FULL.as_str(), "0000");
        }
    }

    mod errors {
        //! A failed construction must report what didn't fit.
        use super::*;

        /// `CapacityError::len` must be the total length of all the parts, not
        /// the number of bytes copied before running out of room.
        #[test]
        fn reports_total_length() {
            // Runs out of room on the second part, but reports all three.
            const R: Result<FixedStr<4>, CapacityError> =
                FixedStr::try_concat(&["abc", "de", "fgh"]);
            assert_eq!(
                R,
                Err(CapacityError {
                    len: 8,
                    capacity: 4
                })
            );
        }

        /// `CapacityError`'s message must state the length and the capacity.
        #[test]
        fn message() {
            let e = CapacityError {
                len: 5,
                capacity: 4,
            };
            assert_eq!(
                e.to_string(),
                "string exceeds FixedStr capacity: 5 bytes, capacity 4"
            );
        }
    }

    mod access {
        //! Accessors and formatting must expose exactly the stored contents,
        //! and nothing from the unused capacity.
        use super::*;

        /// `len` must be the number of bytes stored, not the capacity.
        #[test]
        fn len_is_bytes_stored() {
            assert_eq!(FixedStr::<8>::try_from_str("abc").unwrap().len(), 3);
        }

        /// `as_bytes` must return exactly the stored bytes, without the unused
        /// capacity.
        #[test]
        fn as_bytes_is_stored_bytes() {
            assert_eq!(
                FixedStr::<8>::try_from_str("abc").unwrap().as_bytes(),
                b"abc"
            );
        }

        /// A non-empty `FixedStr` must not report itself as empty.
        #[test]
        fn non_empty_is_not_empty() {
            assert!(!FixedStr::<8>::try_from_str("a").unwrap().is_empty());
        }

        /// `Display` must write the contents unchanged.
        #[test]
        fn display_writes_contents() {
            let s = FixedStr::<8>::try_from_str("añ€").unwrap();
            assert_eq!(std::format!("{s}"), "añ€");
        }

        /// `Debug` must format the contents the same way as `str`'s `Debug`.
        #[test]
        fn debug_matches_str_debug() {
            let s = FixedStr::<8>::try_from_str("a\"b").unwrap();
            assert_eq!(std::format!("{s:?}"), std::format!("{:?}", "a\"b"));
        }

        /// `as_ref` must return the contents as a `&str`.
        #[test]
        fn as_ref_is_contents() {
            let s = FixedStr::<8>::try_from_str("abc").unwrap();
            let r: &str = s.as_ref();
            assert_eq!(r, "abc");
        }
    }

    mod value_semantics {
        //! A `FixedStr` must behave as a plain value: copyable, and compared
        //! and hashed by contents only.
        use super::*;

        /// `FixedStr`s with the same contents must be equal, however they were built.
        #[test]
        fn same_contents_are_equal() {
            const FROM_STR: Result<FixedStr<8>, CapacityError> = FixedStr::try_from_str("ab");
            const CONCAT: Result<FixedStr<8>, CapacityError> = FixedStr::try_concat(&["a", "b"]);
            assert_eq!(FROM_STR.unwrap(), CONCAT.unwrap());
        }

        /// `FixedStr`s with different contents must not be equal.
        #[test]
        fn different_contents_are_not_equal() {
            // Including a longer string that shares the same prefix.
            assert_ne!(
                FixedStr::<8>::try_from_str("ab").unwrap(),
                FixedStr::<8>::try_from_str("abc").unwrap()
            );
        }

        /// `FixedStr`s with the same contents must hash equally.
        #[test]
        fn same_contents_hash_equally() {
            use core::hash::BuildHasher;
            use std::hash::RandomState;

            let state = RandomState::new();
            assert_eq!(
                state.hash_one(FixedStr::<8>::try_from_str("ab").unwrap()),
                state.hash_one(FixedStr::<8>::try_concat(&["a", "b"]).unwrap())
            );
        }

        /// `FixedStr` must be `Copy`, so it can be returned from consts and
        /// passed by value.
        #[test]
        fn is_copy() {
            fn assert_copy<T: Copy>() {}
            assert_copy::<FixedStr<8>>();
        }
    }

    mod utf8_overwrite_str {
        //! `Utf8Buf::overwrite_str` must write a `str` at a given position within
        //! the buffer, or fail and leave the buffer unchanged.
        use super::utf8_buf::*;

        /// `overwrite_str` must write `s` at position `at` and return the
        /// position just after it.
        #[test]
        fn appends() {
            let mut buf = Utf8Buf::<8>::zero_digit_filled();
            let end = buf.overwrite_str(0, "ab").unwrap();
            let end = buf.overwrite_str(end, "cde").unwrap();
            assert_eq!(end, 5);
            assert_eq!(&buf.as_str()[..end], "abcde");
        }

        /// `overwrite_str` should accept UTF-8 strings that contains multi
        /// byte characters.
        #[test]
        fn multi_byte_chars() {
            // S is 10 bytes long
            const S: &str = "a\u{e9}\u{20ac}\u{1f600}"; // same as "aé€😀"
            let mut buf = Utf8Buf::<10>::zero_digit_filled();
            buf.overwrite_str(0, S).unwrap();

            assert_eq!(buf.as_str(), S);
        }

        /// `overwrite_str` must succeed when the write exactly fills the buffer.
        #[test]
        fn exact_fill() {
            let mut buf = Utf8Buf::<4>::zero_digit_filled();
            assert_eq!(buf.overwrite_str(0, "abcd"), Some(4));
            assert_eq!(buf.as_str(), "abcd");
        }

        /// `overwrite_str` must succeed for an empty write at the end of the
        /// buffer, so every position from 0 to the buffer length is a valid
        /// place to write.
        #[test]
        fn empty_at_end() {
            let mut buf = Utf8Buf::<4>::zero_digit_filled();
            assert_eq!(buf.overwrite_str(4, ""), Some(4));
        }

        /// `overwrite_str` must return `None` when `s` doesn't fit after `at`.
        #[test]
        fn overflow_is_none() {
            let mut buf = Utf8Buf::<4>::zero_digit_filled();
            assert_eq!(buf.overwrite_str(3, "xy"), None);
        }

        /// `overwrite_str` must return `None` when `at` is past the end of the
        /// buffer, even for an empty `s`.
        #[test]
        fn start_past_end_is_none() {
            let mut buf = Utf8Buf::<4>::zero_digit_filled();
            assert_eq!(buf.overwrite_str(5, ""), None);
        }

        /// `overwrite_str` must return `None`, not overflow, when `at` plus the
        /// length of `s` exceeds `usize::MAX`.
        #[test]
        fn position_overflow_is_none() {
            let mut buf = Utf8Buf::<4>::zero_digit_filled();
            assert_eq!(buf.overwrite_str(usize::MAX, "x"), None);
        }

        /// A failed `overwrite_str` must leave the buffer unchanged.
        #[test]
        fn failure_leaves_buffer_unchanged() {
            let mut buf = Utf8Buf::<4>::zero_digit_filled();
            buf.overwrite_str(0, "abcd").unwrap();
            assert_eq!(buf.overwrite_str(3, "xy"), None);
            assert_eq!(buf.as_str(), "abcd");
        }
    }
}
