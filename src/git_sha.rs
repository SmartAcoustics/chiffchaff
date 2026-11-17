//! [`GitSha`]: a validated git commit hash.

use core::fmt;

use snafu::Snafu;

use crate::FixedStr;

/// Why a string couldn't be parsed as a [`GitSha`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Snafu)]
pub enum GitShaError {
    /// Not 40 (SHA-1) or 64 (SHA-256) characters long.
    #[snafu(display("{}, got {len}", self.const_message()))]
    BadLength { len: usize },
    /// The character at this byte index isn't a hex digit.
    #[snafu(display("{} at index {index}", self.const_message()))]
    NotHex { index: usize },
    /// An error was encountered creating the internal FixedStr. This is a bug.
    CreatingFixedStr,
    /// The internal conversion of what should be a byte array containing a UTF-8 string failed.
    /// This is a bug because that conversion should always succeed.
    InternallyConvertingBytesToUtf8,
}

impl GitShaError {
    /// This error's message without runtime details, usable in const
    /// contexts. [`Display`](fmt::Display) is this plus the details.
    pub const fn const_message(&self) -> &'static str {
        match self {
            GitShaError::BadLength { .. } => {
                "git SHA must be a full 40 (SHA-1) or 64 (SHA-256) hex characters"
            }
            GitShaError::NotHex { .. } => "git SHA contains a non-hex character",
            GitShaError::CreatingFixedStr => "bug creating internal FixedStr",
            GitShaError::InternallyConvertingBytesToUtf8 => "bug creating internal utf-8 string",
        }
    }
}

/// Exactly `N` hex digits, stored as lowercase ASCII text.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct HexStr<const N: usize>(FixedStr<N>); // Invariant: all lowercase hex digits.

impl<const N: usize> HexStr<N> {
    /// Validates `s` as exactly `N` hex digits. Uppercase is accepted and
    /// normalised to lowercase.
    pub const fn try_from_str(s: &str) -> Result<Self, GitShaError> {
        if s.len() != N {
            return Err(GitShaError::BadLength { len: s.len() });
        }

        let mut s_bytes = s.as_bytes();
        let mut holding = [0u8; N];

        let mut index = 0;

        while let Some((&c, s_rest)) = s_bytes.split_first() {
            let lower_c = match c {
                b'0'..=b'9' | b'a'..=b'f' => c,
                b'A'..=b'F' => c.to_ascii_lowercase(),
                _ => return Err(GitShaError::NotHex { index }),
            };
            holding[index] = lower_c;

            s_bytes = s_rest;
            index += 1;
        }

        let canonical_hex_str = match core::str::from_utf8(&holding) {
            Ok(s) => s,
            Err(_) => return Err(GitShaError::InternallyConvertingBytesToUtf8),
        };

        let fixed_str: FixedStr<N> = match FixedStr::try_from_str(canonical_hex_str) {
            Ok(fixed_str) => fixed_str,
            Err(_) => return Err(GitShaError::CreatingFixedStr),
        };

        Ok(HexStr(fixed_str))
    }

    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<const N: usize> HexStr<N> {
    /// All zeros, for the worst-case encoding check.
    pub(crate) const ZERO: Self = Self(FixedStr::<N>::zero_digit_filled());
}

impl<const N: usize> fmt::Debug for HexStr<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl<const N: usize> fmt::Display for HexStr<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A full git commit hash: SHA-1 (current default) or SHA-256.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GitSha {
    Sha1(HexStr<40>),
    Sha256(HexStr<64>),
}

impl GitSha {
    /// Parses a full hex hash, choosing the enum variant from its length.
    pub const fn try_from_hex(s: &str) -> Result<Self, GitShaError> {
        match s.len() {
            40 => match HexStr::try_from_str(s) {
                Ok(h) => Ok(GitSha::Sha1(h)),
                Err(e) => Err(e),
            },
            64 => match HexStr::try_from_str(s) {
                Ok(h) => Ok(GitSha::Sha256(h)),
                Err(e) => Err(e),
            },
            len => Err(GitShaError::BadLength { len }),
        }
    }

    pub const fn as_str(&self) -> &str {
        match self {
            GitSha::Sha1(h) => h.as_str(),
            GitSha::Sha256(h) => h.as_str(),
        }
    }
}

impl fmt::Display for GitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    //! The crate must provide `GitSha`: a full git commit hash, SHA-1 or
    //! SHA-256, validated as hex, stored as lowercase text, and constructible
    //! in a const context.
    //!
    //! Tests build the values they check in `const` items, so each one also
    //! shows that the constructor it uses works at compile time.

    extern crate std;
    use super::*;
    use std::string::ToString;

    const SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    const SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const SHA256_UPPER: &str = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";

    type ShaResult = Result<GitSha, GitShaError>;

    mod algorithm {
        //! The hash algorithm must be chosen from the length of the hash.
        use super::*;

        /// A 40-digit hash must be a SHA-1 hash.
        #[test]
        fn forty_digits_is_sha1() {
            const SHA: ShaResult = GitSha::try_from_hex(SHA1);
            assert!(matches!(SHA, Ok(GitSha::Sha1(_))));
        }

        /// A 64-digit hash must be a SHA-256 hash.
        #[test]
        fn sixty_four_digits_is_sha256() {
            const SHA: ShaResult = GitSha::try_from_hex(SHA256);
            assert!(matches!(SHA, Ok(GitSha::Sha256(_))));
        }

        /// A hash of any other length must be rejected with its length,
        /// including abbreviated hashes and ones a digit short or over.
        #[test]
        fn other_lengths_are_rejected() {
            const EMPTY: ShaResult = GitSha::try_from_hex("");
            const SHORT: ShaResult = GitSha::try_from_hex("da39a3e");
            const SHA1_MINUS_ONE: ShaResult =
                GitSha::try_from_hex("da39a3ee5e6b4b0d3255bfef95601890afd8070");
            const SHA1_PLUS_ONE: ShaResult =
                GitSha::try_from_hex("da39a3ee5e6b4b0d3255bfef95601890afd807090");
            const SHA256_MINUS_ONE: ShaResult = GitSha::try_from_hex(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85",
            );
            assert_eq!(EMPTY, Err(GitShaError::BadLength { len: 0 }));
            assert_eq!(SHORT, Err(GitShaError::BadLength { len: 7 }));
            assert_eq!(SHA1_MINUS_ONE, Err(GitShaError::BadLength { len: 39 }));
            assert_eq!(SHA1_PLUS_ONE, Err(GitShaError::BadLength { len: 41 }));
            assert_eq!(SHA256_MINUS_ONE, Err(GitShaError::BadLength { len: 63 }));
        }
    }

    mod hex_validation {
        //! A hash must contain only hex digits.
        use super::*;

        /// Every hex digit, in either case, must be accepted.
        #[test]
        fn every_hex_digit_is_accepted() {
            const DIGITS: Result<HexStr<22>, GitShaError> =
                HexStr::try_from_str("0123456789abcdefABCDEF");
            assert!(DIGITS.is_ok());
        }

        /// A non-hex character must be rejected with its byte index.
        #[test]
        fn non_hex_is_rejected_with_its_index() {
            const FIRST: ShaResult =
                GitSha::try_from_hex("ga39a3ee5e6b4b0d3255bfef95601890afd80709");
            const LAST: ShaResult =
                GitSha::try_from_hex("da39a3ee5e6b4b0d3255bfef95601890afd8070z");
            assert_eq!(FIRST, Err(GitShaError::NotHex { index: 0 }));
            assert_eq!(LAST, Err(GitShaError::NotHex { index: 39 }));
        }

        /// A non-ASCII character must be rejected, even when the hash has the
        /// right length in bytes.
        #[test]
        fn non_ascii_is_rejected() {
            // 38 hex digits plus "é" (2 bytes) is 40 bytes.
            const SHA: ShaResult = GitSha::try_from_hex("da39a3ee5e6b4b0d3255bfef95601890afd807é");
            assert_eq!(SHA, Err(GitShaError::NotHex { index: 38 }));
        }
    }

    mod normalisation {
        //! A hash must be stored as lowercase hex, whatever case it was given in.
        use super::*;

        /// An uppercase hash must be stored in lowercase.
        #[test]
        fn uppercase_is_stored_lowercase() {
            const SHA: ShaResult = GitSha::try_from_hex(SHA256_UPPER);
            assert_eq!(SHA.unwrap().as_str(), SHA256);
        }

        /// A lowercase hash must be stored unchanged.
        #[test]
        fn lowercase_is_unchanged() {
            const SHA: ShaResult = GitSha::try_from_hex(SHA1);
            assert_eq!(SHA.unwrap().as_str(), SHA1);
        }

        /// A mixed-case hash must be stored in lowercase.
        #[test]
        fn mixed_case_is_stored_lowercase() {
            let upper_sha1 = SHA1.to_uppercase();
            let sha = GitSha::try_from_hex(&upper_sha1);
            assert_eq!(sha.unwrap().as_str(), SHA1);
        }
    }

    mod hex_str {
        //! A `HexStr<N>` must hold exactly `N` hex digits.
        use super::*;

        /// A string containing invalid hex digits must return a GitShaError::NotHex
        /// error
        #[test]
        fn invalid_hex_characters_rejected() {
            const BAD: Result<HexStr<4>, GitShaError> = HexStr::try_from_str("bejf");
            assert_eq!(BAD, Err(GitShaError::NotHex { index: 2 }));
        }

        /// A string of exactly `N` hex digits must be accepted unchanged.
        #[test]
        fn exact_length_is_accepted() {
            const HEX: Result<HexStr<4>, GitShaError> = HexStr::try_from_str("beef");
            assert_eq!(HEX.unwrap().as_str(), "beef");
        }

        /// A hex string must always be stored in lower, converted as necessary
        #[test]
        fn stored_as_lowercase() {
            const HEX: Result<HexStr<4>, GitShaError> = HexStr::try_from_str("BeeF");
            assert_eq!(HEX.unwrap().as_str(), "beef");
        }

        /// A string of any other length must be rejected with its length.
        #[test]
        fn other_lengths_are_rejected() {
            const SHORT: Result<HexStr<4>, GitShaError> = HexStr::try_from_str("bee");
            const LONG: Result<HexStr<4>, GitShaError> = HexStr::try_from_str("beefs");
            assert_eq!(SHORT, Err(GitShaError::BadLength { len: 3 }));
            assert_eq!(LONG, Err(GitShaError::BadLength { len: 5 }));
        }

        /// `HexStr::ZERO` must be `N` zero digits.
        #[test]
        fn zero_is_all_zero_digits() {
            const ZERO: HexStr<4> = HexStr::ZERO;
            assert_eq!(ZERO.as_str(), "0000");
        }
    }

    mod equality {
        //! Hashes must be compared by their lowercase digits.
        use super::*;

        /// The same hash given in different cases must be equal.
        #[test]
        fn same_hash_in_any_case_is_equal() {
            const UPPER: ShaResult = GitSha::try_from_hex(SHA256_UPPER);
            const LOWER: ShaResult = GitSha::try_from_hex(SHA256);
            assert_eq!(UPPER.unwrap(), LOWER.unwrap());
        }

        /// Different hashes must not be equal.
        #[test]
        fn different_hashes_are_not_equal() {
            const A: ShaResult = GitSha::try_from_hex(SHA1);
            const B: ShaResult = GitSha::try_from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80708");
            assert_ne!(A.unwrap(), B.unwrap());
        }
    }

    mod formatting {
        //! Hashes must format as their lowercase hex digits.
        use super::*;

        /// `GitSha`'s `Display` must write the lowercase hex digits.
        #[test]
        fn git_sha_display_is_lowercase_hex() {
            const SHA: ShaResult = GitSha::try_from_hex(SHA256_UPPER);
            assert_eq!(SHA.unwrap().to_string(), SHA256);
        }

        /// `HexStr`'s `Display` must write its digits.
        #[test]
        fn hex_str_display_is_digits() {
            const HEX: Result<HexStr<4>, GitShaError> = HexStr::try_from_str("BEEF");
            assert_eq!(HEX.unwrap().to_string(), "beef");
        }

        /// `HexStr`'s `Debug` must format its digits the same way as `str`'s
        /// `Debug`.
        #[test]
        fn hex_str_debug_matches_str_debug() {
            const HEX: Result<HexStr<4>, GitShaError> = HexStr::try_from_str("beef");
            assert_eq!(
                std::format!("{:?}", HEX.unwrap()),
                std::format!("{:?}", "beef")
            );
        }
    }

    mod errors {
        //! A `GitShaError` must say what was wrong with the hash.
        use super::*;

        /// A `BadLength` error's message must give the length found.
        #[test]
        fn bad_length_message() {
            assert_eq!(
                GitShaError::BadLength { len: 7 }.to_string(),
                "git SHA must be a full 40 (SHA-1) or 64 (SHA-256) hex characters, got 7"
            );
        }

        /// A `NotHex` error's message must give the index of the bad character.
        #[test]
        fn not_hex_message() {
            assert_eq!(
                GitShaError::NotHex { index: 3 }.to_string(),
                "git SHA contains a non-hex character at index 3"
            );
        }

        /// Each error's `Display` must start with its `const_message`.
        #[test]
        fn display_starts_with_const_message() {
            for e in [
                GitShaError::BadLength { len: 7 },
                GitShaError::NotHex { index: 3 },
            ] {
                assert!(e.to_string().starts_with(e.const_message()), "{e:?}");
            }
        }
    }
}
