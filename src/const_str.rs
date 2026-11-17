//! Small `const fn` string helpers.

/// A package name: a name that starts with an ASCII letter or `_`, as Cargo
/// requires.
pub(crate) const fn is_package_name_safe(s: &str) -> bool {
    match s.as_bytes().first() {
        Some(&c) => (c.is_ascii_alphabetic() || c == b'_') && is_name_safe(s),
        None => false,
    }
}

/// A binary name: non-empty ASCII letters, digits, `-` and `_`. Cargo allows a
/// binary to start with any of them (`2048` is a valid binary name).
pub(crate) const fn is_name_safe(s: &str) -> bool {
    let mut rest = s.as_bytes();
    if rest.is_empty() {
        return false;
    }
    while let Some((&c, tail)) = rest.split_first() {
        if !(c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
            return false;
        }
        rest = tail;
    }
    true
}

/// A SemVer version: non-empty ASCII letters, digits, `.`, `+` and `-`.
pub(crate) const fn is_version_safe(s: &str) -> bool {
    let mut rest = s.as_bytes();
    if rest.is_empty() {
        return false;
    }
    while let Some((&c, tail)) = rest.split_first() {
        if !(c.is_ascii_alphanumeric() || c == b'.' || c == b'+' || c == b'-') {
            return false;
        }
        rest = tail;
    }
    true
}

/// `==` on `&str` isn't const, so compare bytes.
pub(crate) const fn str_eq(a: &str, b: &str) -> bool {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    while let (Some((x, a_rest)), Some((y, b_rest))) = (a.split_first(), b.split_first()) {
        if *x != *y {
            return false;
        }
        a = a_rest;
        b = b_rest;
    }
    true
}

#[cfg(test)]
// Results are computed in `const` items to show the helpers work at compile
// time, then asserted at runtime so a wrong answer is a test failure rather
// than a compile error.
#[allow(clippy::assertions_on_constants)]
mod tests {
    //! The crate must provide `const fn` helpers for comparing strings and for
    //! checking that a value can be stored in a record field.
    //!
    //! Tests evaluate the helpers in `const` items where they can, so each one
    //! also shows that the helper works at compile time.

    use super::*;

    mod str_eq {
        //! `str_eq` must agree with `==` on `&str`.
        use super::*;

        /// Identical strings must be equal.
        #[test]
        fn identical_strings_are_equal() {
            const EQ: bool = str_eq("my-app", "my-app");
            assert!(EQ);
        }

        /// Empty strings must be equal.
        #[test]
        fn empty_strings_are_equal() {
            const EQ: bool = str_eq("", "");
            assert!(EQ);
        }

        /// Strings of the same length that differ must not be equal.
        #[test]
        fn same_length_different_strings_are_not_equal() {
            const FIRST: bool = str_eq("xbc", "abc");
            const LAST: bool = str_eq("abx", "abc");
            assert!(!FIRST);
            assert!(!LAST);
        }

        /// A string must not equal a longer string that starts with it.
        #[test]
        fn prefix_is_not_equal() {
            const SHORTER: bool = str_eq("ab", "abc");
            const LONGER: bool = str_eq("abc", "ab");
            assert!(!SHORTER);
            assert!(!LONGER);
        }

        /// Comparison must be case-sensitive.
        #[test]
        fn is_case_sensitive() {
            const EQ: bool = str_eq("true", "TRUE");
            assert!(!EQ);
        }

        /// `str_eq` must give the same answer as `==` for any pair of strings.
        #[test]
        fn agrees_with_eq() {
            let strings = ["", "a", "b", "ab", "ba", "abc", "é", "e", "true", "false"];
            for a in strings {
                for b in strings {
                    assert_eq!(str_eq(a, b), a == b, "{a:?} vs {b:?}");
                }
            }
        }
    }

    mod is_package_name_safe {
        //! `is_package_name_safe` must accept exactly the names `is_name_safe`
        //! accepts that start with an ASCII letter or `_`.
        use super::*;

        /// Typical package names must be safe.
        #[test]
        fn typical_package_names_are_safe() {
            const HYPHEN: bool = is_package_name_safe("magpie-version");
            const UNDERSCORE: bool = is_package_name_safe("_private");
            assert!(HYPHEN);
            assert!(UNDERSCORE);
        }

        /// Every ASCII character must be classified as a first character:
        /// letters and `_` are safe, everything else is not.
        #[test]
        fn every_ascii_first_character_is_classified() {
            for byte in 0u8..=127 {
                let c = [byte, b'a'];
                let s = core::str::from_utf8(&c).unwrap();
                let expected = byte.is_ascii_alphabetic() || byte == b'_';
                assert_eq!(is_package_name_safe(s), expected, "byte {byte:#04x}");
            }
        }

        /// A name starting with a digit must not be safe.
        #[test]
        fn leading_digit_is_not_safe() {
            const SAFE: bool = is_package_name_safe("2048");
            assert!(!SAFE);
        }

        /// The empty string must not be safe.
        #[test]
        fn empty_is_not_safe() {
            const SAFE: bool = is_package_name_safe("");
            assert!(!SAFE);
        }

        /// After a safe first character, every ASCII character must be
        /// classified as `is_name_safe` classifies it: letters, digits, `-`
        /// and `_` are safe, everything else is not.
        #[test]
        fn every_ascii_later_character_is_classified() {
            for byte in 0u8..=127 {
                let c = [b'a', byte];
                let s = core::str::from_utf8(&c).unwrap();
                let expected = byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
                assert_eq!(is_package_name_safe(s), expected, "byte {byte:#04x}");
            }
        }
    }

    mod is_name_safe {
        //! `is_name_safe` must accept exactly the non-empty strings of ASCII
        //! letters, digits, `-` and `_`.
        use super::*;

        /// Typical package and binary names must be safe.
        #[test]
        fn typical_names_are_safe() {
            const PACKAGE: bool = is_name_safe("magpie-version");
            const BINARY: bool = is_name_safe("my_cli2");
            const LEADING_DIGIT: bool = is_name_safe("2048");
            assert!(PACKAGE);
            assert!(BINARY);
            assert!(LEADING_DIGIT);
        }

        /// Every ASCII letter, digit, `-` and `_` must be safe on its own, and
        /// every other ASCII character must not be.
        #[test]
        fn every_ascii_character_is_classified() {
            for byte in 0u8..=127 {
                let c = [byte];
                let s = core::str::from_utf8(&c).unwrap();
                let expected = byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
                assert_eq!(is_name_safe(s), expected, "byte {byte:#04x}");
            }
        }

        /// The empty string must not be safe.
        #[test]
        fn empty_is_not_safe() {
            const SAFE: bool = is_name_safe("");
            assert!(!SAFE);
        }

        /// A disallowed character anywhere in the string must make it unsafe.
        #[test]
        fn disallowed_character_anywhere_is_not_safe() {
            const START: bool = is_name_safe(".app");
            const MIDDLE: bool = is_name_safe("my+app");
            const END: bool = is_name_safe("app;");
            assert!(!START);
            assert!(!MIDDLE);
            assert!(!END);
        }

        /// Non-ASCII letters must not be safe.
        #[test]
        fn non_ascii_is_not_safe() {
            const SAFE: bool = is_name_safe("café");
            assert!(!SAFE);
        }
    }

    mod is_version_safe {
        //! `is_version_safe` must accept exactly the non-empty strings of ASCII
        //! letters, digits, `.`, `+` and `-`.
        use super::*;

        /// Typical versions, with pre-release and build metadata, must be safe.
        #[test]
        fn typical_versions_are_safe() {
            const RELEASE: bool = is_version_safe("1.2.3");
            const FULL: bool = is_version_safe("1.2.3-rc.1+build.5");
            assert!(RELEASE);
            assert!(FULL);
        }

        /// Every ASCII letter, digit, `.`, `+` and `-` must be safe on its own,
        /// and every other ASCII character must not be.
        #[test]
        fn every_ascii_character_is_classified() {
            for byte in 0u8..=127 {
                let c = [byte];
                let s = core::str::from_utf8(&c).unwrap();
                let expected =
                    byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'+' || byte == b'-';
                assert_eq!(is_version_safe(s), expected, "byte {byte:#04x}");
            }
        }

        /// The empty string must not be safe.
        #[test]
        fn empty_is_not_safe() {
            const SAFE: bool = is_version_safe("");
            assert!(!SAFE);
        }

        /// A disallowed character anywhere in the string must make it unsafe.
        #[test]
        fn disallowed_character_anywhere_is_not_safe() {
            const START: bool = is_version_safe("_1.2.3");
            const MIDDLE: bool = is_version_safe("1.2/3");
            const END: bool = is_version_safe("1.2.3;");
            assert!(!START);
            assert!(!MIDDLE);
            assert!(!END);
        }

        /// Non-ASCII characters must not be safe.
        #[test]
        fn non_ascii_is_not_safe() {
            const SAFE: bool = is_version_safe("1.2.3-é");
            assert!(!SAFE);
        }
    }
}
