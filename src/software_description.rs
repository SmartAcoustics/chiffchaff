//! [`SoftwareDescription`]: building, encoding and parsing its [`Record`]s.

use core::fmt;

use snafu::{OptionExt, ResultExt, Snafu, ensure};

use crate::const_str::{is_name_safe, is_package_name_safe, is_version_safe, str_eq};
use crate::fixed_str::{CapacityError, FixedStr};
use crate::git_sha::{GitSha, GitShaError, HexStr};

/// Maximum length of a DNS character-string (RFC 1035 §3.3).
pub const DNS_STRING_MAX: usize = 255;

/// The encoded form of a [`SoftwareDescription`], sized to a DNS character-string.
pub type Record = FixedStr<DNS_STRING_MAX>;

/// The value vergen emits in place of real data when configured with
/// `idempotent()` or `default_on_error()`. Rejected by [`SoftwareDescription::try_new`].
pub const VERGEN_PLACEHOLDER: &str = "VERGEN_IDEMPOTENT_OUTPUT";

/// Why a [`SoftwareDescription`] couldn't be built ([`SoftwareDescription::try_new`]) or parsed
/// ([`SoftwareDescription::parse`]).
///
/// A value that can't be converted into its field gives a `Converting...Field`
/// error naming the field, whose source is a [`StrToFixedFieldError`] saying
/// what was wrong with the value, whose source in turn (where there is one) is
/// the underlying [`CapacityError`] or [`GitShaError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Snafu)]
pub enum SoftwareDescriptionError {
    /// A git field holds vergen's placeholder rather than real data.
    #[snafu(display("{}", self.const_message()))]
    VergenPlaceholder,

    /// A record is longer than [`DNS_STRING_MAX`] bytes.
    #[snafu(display("{}: got {len} bytes", self.const_message()))]
    TooLong { len: usize },
    /// A record's raw bytes aren't valid UTF-8.
    #[snafu(display("{}", self.const_message()))]
    NotUtf8 { source: core::str::Utf8Error },
    /// A record lacks its package field (`p=`), or it's out of order.
    #[snafu(display("{}", self.const_message()))]
    MissingPackageField,
    /// A record lacks its package type field (`t=`), or it's out of order.
    #[snafu(display("{}", self.const_message()))]
    MissingPackageTypeField,
    /// A record lacks its version field (`v=`), or it's out of order.
    #[snafu(display("{}", self.const_message()))]
    MissingVersionField,
    /// A record lacks its git SHA field (`g=`), or it's out of order.
    #[snafu(display("{}", self.const_message()))]
    MissingGitShaField,
    /// A record lacks its git dirty field (`d=`), or it's out of order.
    #[snafu(display("{}", self.const_message()))]
    MissingGitDirtyField,
    /// A record's package type isn't `l` or `b:<binary name>`.
    #[snafu(display("{}", self.const_message()))]
    InvalidPackageType,

    /// The package name can't be converted into its field.
    #[snafu(display("{}", self.const_message()))]
    ConvertingPackageField { source: StrToFixedFieldError },
    /// The binary name can't be converted into its field.
    #[snafu(display("{}", self.const_message()))]
    ConvertingBinaryNameField { source: StrToFixedFieldError },
    /// The version can't be converted into its field.
    #[snafu(display("{}", self.const_message()))]
    ConvertingVersionField { source: StrToFixedFieldError },
    /// The git SHA can't be converted into a [`GitSha`].
    #[snafu(display("{}", self.const_message()))]
    ConvertingGitShaField { source: StrToFixedFieldError },
    /// The git dirty flag can't be converted into a `bool`.
    #[snafu(display("{}", self.const_message()))]
    ConvertingGitDirtyField { source: StrToFixedFieldError },
}

impl SoftwareDescriptionError {
    /// This error's message without runtime details, usable in const
    /// contexts. [`Display`](fmt::Display) is this plus any details.
    pub const fn const_message(&self) -> &'static str {
        match self {
            SoftwareDescriptionError::VergenPlaceholder => {
                "git info is vergen's placeholder; turn off vergen's idempotent/default_on_error modes"
            }
            SoftwareDescriptionError::TooLong { .. } => "record exceeds DNS_STRING_MAX bytes",
            SoftwareDescriptionError::NotUtf8 { .. } => "record is not valid UTF-8",
            SoftwareDescriptionError::MissingPackageField => {
                "record is missing the package field (p=), or it's out of order"
            }
            SoftwareDescriptionError::MissingPackageTypeField => {
                "record is missing the package type field (t=), or it's out of order"
            }
            SoftwareDescriptionError::MissingVersionField => {
                "record is missing the version field (v=), or it's out of order"
            }
            SoftwareDescriptionError::MissingGitShaField => {
                "record is missing the git SHA field (g=), or it's out of order"
            }
            SoftwareDescriptionError::MissingGitDirtyField => {
                "record is missing the git dirty field (d=), or it's out of order"
            }
            SoftwareDescriptionError::InvalidPackageType => {
                "package type must be 'l' (library) or 'b:<binary name>' (binary)"
            }
            SoftwareDescriptionError::ConvertingPackageField { .. } => "invalid package name",
            SoftwareDescriptionError::ConvertingBinaryNameField { .. } => "invalid binary name",
            SoftwareDescriptionError::ConvertingVersionField { .. } => "invalid package version",
            SoftwareDescriptionError::ConvertingGitShaField { .. } => "invalid git SHA",
            SoftwareDescriptionError::ConvertingGitDirtyField { .. } => "invalid git dirty flag",
        }
    }

    /// The field conversion error this error wraps, if any.
    const fn field_error(&self) -> Option<&StrToFixedFieldError> {
        match self {
            SoftwareDescriptionError::ConvertingPackageField { source }
            | SoftwareDescriptionError::ConvertingBinaryNameField { source }
            | SoftwareDescriptionError::ConvertingVersionField { source }
            | SoftwareDescriptionError::ConvertingGitShaField { source }
            | SoftwareDescriptionError::ConvertingGitDirtyField { source } => Some(source),
            _ => None,
        }
    }

    /// The `const_message` of this error and of every error in its source
    /// chain, joined with `": "`. Usable in const contexts (it's what
    /// [`get_crate_version!`](crate::get_crate_version!) reports as a compile
    /// error).
    ///
    /// `NotUtf8`'s source, `core::str::Utf8Error`, has no const message, so its
    /// report stops at `NotUtf8`.
    pub const fn const_report(&self) -> FixedStr<{ SoftwareDescriptionError::REPORT_CAP }> {
        let message = self.const_message();
        let report = match self.field_error() {
            None => FixedStr::try_from_str(message),
            Some(field) => match field.cause_const_message() {
                None => FixedStr::try_concat(&[message, ": ", field.const_message()]),
                Some(cause) => {
                    FixedStr::try_concat(&[message, ": ", field.const_message(), ": ", cause])
                }
            },
        };
        match report {
            Ok(report) => report,
            // Unreachable: a test checks every report fits.
            Err(_) => FixedStr::EMPTY,
        }
    }

    /// Capacity of [`const_report`](Self::const_report)'s output.
    pub const REPORT_CAP: usize = 256;
}

/// Why a string couldn't be converted into a field of a [`SoftwareDescription`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Snafu)]
pub enum StrToFixedFieldError {
    /// The package name is empty, starts with something other than a letter
    /// or `_`, or contains a character a package name can't hold.
    #[snafu(display("{}", self.const_message()))]
    InvalidPackageName,
    /// The binary name is empty or contains a character a binary name can't
    /// hold.
    #[snafu(display("{}", self.const_message()))]
    InvalidName,
    /// The version is empty or contains a character a version can't hold.
    #[snafu(display("{}", self.const_message()))]
    InvalidVersion,
    /// The string is longer than the field's capacity.
    #[snafu(display("{}", self.const_message()))]
    FieldTooLong { source: CapacityError },
    /// The string isn't a valid git SHA.
    #[snafu(display("{}", self.const_message()))]
    InvalidGitSha { source: GitShaError },
    /// The string isn't one of the two accepted spellings of a flag.
    #[snafu(display("{}", self.const_message()))]
    InvalidBool,
}

impl StrToFixedFieldError {
    /// This error's message without runtime details, usable in const
    /// contexts. [`Display`](fmt::Display) is this plus any details.
    pub const fn const_message(&self) -> &'static str {
        match self {
            StrToFixedFieldError::InvalidPackageName => {
                "must be an ASCII letter or '_', then ASCII letters, digits, '-' or '_'"
            }
            StrToFixedFieldError::InvalidName => {
                "must be non-empty ASCII letters, digits, '-' or '_'"
            }
            StrToFixedFieldError::InvalidVersion => {
                "must be non-empty ASCII letters, digits, '.', '+' or '-'"
            }
            StrToFixedFieldError::FieldTooLong { .. } => "too long for its field",
            StrToFixedFieldError::InvalidGitSha { .. } => "not a full hex git SHA",
            StrToFixedFieldError::InvalidBool => {
                "must be \"true\" or \"false\" (\"1\" or \"0\" in a record)"
            }
        }
    }

    /// The `const_message` of this error's source, if it has one.
    const fn cause_const_message(&self) -> Option<&'static str> {
        match self {
            StrToFixedFieldError::FieldTooLong { source } => Some(source.const_message()),
            StrToFixedFieldError::InvalidGitSha { source } => Some(source.const_message()),
            StrToFixedFieldError::InvalidPackageName
            | StrToFixedFieldError::InvalidName
            | StrToFixedFieldError::InvalidVersion
            | StrToFixedFieldError::InvalidBool => None,
        }
    }
}

/// Whether a [`SoftwareDescription`] was built into a library or a binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PackageType {
    Library,
    /// A binary target, with its name (which can differ from the package's).
    Binary(FixedStr<{ SoftwareDescription::BINARY_CAP }>),
}

/// Identifies exactly which build of a piece of software is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoftwareDescription {
    pub package: FixedStr<{ SoftwareDescription::PACKAGE_CAP }>,
    pub package_type: PackageType,
    pub version: FixedStr<{ SoftwareDescription::VERSION_CAP }>,
    pub git_sha: GitSha,
    /// The working tree had uncommitted changes to tracked files.
    pub git_dirty: bool,
}

impl SoftwareDescription {
    /// Capacity of [`package`](Self::package). crates.io limits package names to 64 characters.
    pub const PACKAGE_CAP: usize = 64;

    /// Capacity of [`version`](Self::version).
    pub const VERSION_CAP: usize = 64;

    /// Capacity of a [`PackageType::Binary`] name.
    pub const BINARY_CAP: usize = 32;

    /// Builds a `SoftwareDescription` from raw strings, validating everything.
    ///
    /// - `package`: an ASCII letter or `_`, then ASCII letters, digits, `-`
    ///   and `_`, within [`PACKAGE_CAP`](Self::PACKAGE_CAP).
    /// - `version`: non-empty ASCII letters, digits, `.`, `+` and `-`, within
    ///   [`VERSION_CAP`](Self::VERSION_CAP).
    /// - `git_sha`: a full hex SHA-1 or SHA-256.
    /// - `git_dirty`: `"true"` or `"false"`.
    /// - `binary`: the binary target's name, or `None` for a library. Same
    ///   rules as `package`, except it may also start with a digit or `-`,
    ///   within [`BINARY_CAP`](Self::BINARY_CAP).
    ///
    /// vergen's placeholder value is rejected for both git fields.
    pub const fn try_new(
        package: &str,
        version: &str,
        git_sha: &str,
        git_dirty: &str,
        binary: Option<&str>,
    ) -> Result<Self, SoftwareDescriptionError> {
        if str_eq(git_sha, VERGEN_PLACEHOLDER) || str_eq(git_dirty, VERGEN_PLACEHOLDER) {
            return Err(SoftwareDescriptionError::VergenPlaceholder);
        }
        let package = match PreFixedStr(package).to_package_fixed_str() {
            Ok(v) => v,
            Err(source) => return Err(SoftwareDescriptionError::ConvertingPackageField { source }),
        };
        let package_type = match binary {
            None => PackageType::Library,
            Some(binary) => match PreFixedStr(binary).to_name_fixed_str() {
                Ok(v) => PackageType::Binary(v),
                Err(source) => {
                    return Err(SoftwareDescriptionError::ConvertingBinaryNameField { source });
                }
            },
        };
        let version = match PreFixedStr(version).to_version_fixed_str() {
            Ok(v) => v,
            Err(source) => return Err(SoftwareDescriptionError::ConvertingVersionField { source }),
        };
        let git_sha = match PreFixedStr(git_sha).to_git_sha() {
            Ok(v) => v,
            Err(source) => return Err(SoftwareDescriptionError::ConvertingGitShaField { source }),
        };
        let git_dirty = match PreFixedStr(git_dirty).to_bool("true", "false") {
            Ok(v) => v,
            Err(source) => {
                return Err(SoftwareDescriptionError::ConvertingGitDirtyField { source });
            }
        };
        Ok(Self {
            package,
            package_type,
            version,
            git_sha,
            git_dirty,
        })
    }

    /// Encodes as `p=<package>;t=<l|b:binary>;v=<version>;g=<sha>;d=<0|1>`.
    pub const fn encode(&self) -> Record {
        match self.try_encode() {
            Ok(record) => record,
            // Unreachable: the worst-case check below proves every
            // `SoftwareDescription` fits.
            Err(_) => Record::EMPTY,
        }
    }

    const fn try_encode(&self) -> Result<Record, CapacityError> {
        let (package_type, binary) = match &self.package_type {
            PackageType::Library => (";t=l", ""),
            PackageType::Binary(binary) => (";t=b:", binary.as_str()),
        };
        Record::try_concat(&[
            "p=",
            self.package.as_str(),
            package_type,
            binary,
            ";v=",
            self.version.as_str(),
            ";g=",
            self.git_sha.as_str(),
            if self.git_dirty { ";d=1" } else { ";d=0" },
        ])
    }
}

// Proves every `SoftwareDescription` encodes into a `Record`. The output length only
// grows with each field, so if the largest possible value fits, all do.
const _: () = {
    let worst = SoftwareDescription {
        package: FixedStr::zero_digit_filled(),
        package_type: PackageType::Binary(FixedStr::zero_digit_filled()),
        version: FixedStr::zero_digit_filled(),
        git_sha: GitSha::Sha256(HexStr::ZERO),
        git_dirty: true,
    };
    assert!(
        worst.try_encode().is_ok(),
        "SoftwareDescription field capacities allow a record longer than DNS_STRING_MAX"
    );
};

impl SoftwareDescription {
    /// Parses a record produced by [`encode`](Self::encode).
    ///
    /// The five fields must appear in exactly the order `encode` writes them.
    /// Anything after the dirty flag is ignored, so a future version can append
    /// fields without breaking this parser. A trailing `;` is accepted.
    pub fn parse(record: &str) -> Result<Self, SoftwareDescriptionError> {
        ensure!(
            record.len() <= DNS_STRING_MAX,
            TooLongSnafu { len: record.len() }
        );
        Self::parse_fields(record)
    }

    /// Like [`parse`](Self::parse), for the raw bytes of a DNS TXT
    /// character-string.
    pub fn parse_bytes(record: &[u8]) -> Result<Self, SoftwareDescriptionError> {
        ensure!(
            record.len() <= DNS_STRING_MAX,
            TooLongSnafu { len: record.len() }
        );
        Self::parse_fields(core::str::from_utf8(record).context(NotUtf8Snafu)?)
    }

    /// Parses `p=<package>;t=<l|b:binary>;v=<version>;g=<sha>;d=<0|1>`, in
    /// that order, ignoring anything after it.
    ///
    /// This only splits the record into its fields; [`try_new`](Self::try_new)
    /// validates them, so parsed and compile-time descriptions follow the same
    /// rules.
    fn parse_fields(record: &str) -> Result<Self, SoftwareDescriptionError> {
        let mut parts = record.split_terminator(';');
        let mut field = |key| parts.next().and_then(|part| part.strip_prefix(key));

        let package = field("p=").context(MissingPackageFieldSnafu)?;
        let package_type = field("t=").context(MissingPackageTypeFieldSnafu)?;
        let version = field("v=").context(MissingVersionFieldSnafu)?;
        let git_sha = field("g=").context(MissingGitShaFieldSnafu)?;
        let git_dirty = field("d=").context(MissingGitDirtyFieldSnafu)?;

        let binary = if package_type == "l" {
            None
        } else {
            match package_type.split_once(':') {
                Some(("b", binary)) => Some(binary),
                _ => return InvalidPackageTypeSnafu.fail(),
            }
        };
        // The record spells the flag `1`/`0`; `try_new` takes `true`/`false`.
        let git_dirty = PreFixedStr(git_dirty)
            .to_bool("1", "0")
            .map(|bool_val| if bool_val { "true" } else { "false" })
            .context(ConvertingGitDirtyFieldSnafu)?;

        Self::try_new(package, version, git_sha, git_dirty, binary)
    }
}

impl core::str::FromStr for SoftwareDescription {
    type Err = SoftwareDescriptionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for SoftwareDescription {
    /// Writes the encoded record.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.encode().as_str())
    }
}

/// A string about to be converted into a [`SoftwareDescription`] field.
///
/// The conversions are `const fn`, so [`SoftwareDescription::try_new`] can use
/// them at compile time and [`SoftwareDescription::parse`] at runtime, giving
/// both the same validation and the same errors.
struct PreFixedStr<'a>(&'a str);

impl PreFixedStr<'_> {
    /// Validates a package name and stores it.
    const fn to_package_fixed_str<const N: usize>(
        &self,
    ) -> Result<FixedStr<N>, StrToFixedFieldError> {
        if !is_package_name_safe(self.0) {
            return Err(StrToFixedFieldError::InvalidPackageName);
        }
        self.to_fixed_str()
    }

    /// Validates a binary name and stores it.
    const fn to_name_fixed_str<const N: usize>(&self) -> Result<FixedStr<N>, StrToFixedFieldError> {
        if !is_name_safe(self.0) {
            return Err(StrToFixedFieldError::InvalidName);
        }
        self.to_fixed_str()
    }

    /// Validates a version and stores it.
    const fn to_version_fixed_str<const N: usize>(
        &self,
    ) -> Result<FixedStr<N>, StrToFixedFieldError> {
        if !is_version_safe(self.0) {
            return Err(StrToFixedFieldError::InvalidVersion);
        }
        self.to_fixed_str()
    }

    /// Stores the string, failing if it's longer than `N` bytes.
    const fn to_fixed_str<const N: usize>(&self) -> Result<FixedStr<N>, StrToFixedFieldError> {
        match FixedStr::try_from_str(self.0) {
            Ok(fixed_str) => Ok(fixed_str),
            Err(source) => Err(StrToFixedFieldError::FieldTooLong { source }),
        }
    }

    /// Converts a full hex git SHA.
    const fn to_git_sha(&self) -> Result<GitSha, StrToFixedFieldError> {
        match GitSha::try_from_hex(self.0) {
            Ok(git_sha) => Ok(git_sha),
            Err(source) => Err(StrToFixedFieldError::InvalidGitSha { source }),
        }
    }

    /// Converts exactly `if_true` to `true` and exactly `if_false` to `false`.
    const fn to_bool(&self, if_true: &str, if_false: &str) -> Result<bool, StrToFixedFieldError> {
        if str_eq(self.0, if_true) {
            Ok(true)
        } else if str_eq(self.0, if_false) {
            Ok(false)
        } else {
            Err(StrToFixedFieldError::InvalidBool)
        }
    }
}

#[cfg(test)]
mod tests {
    //! The crate must provide `SoftwareDescription`: a package's name, package
    //! type, version, git commit and dirty flag, constructible in a const
    //! context, which encodes to a record of at most `DNS_STRING_MAX` bytes
    //! and can be parsed back from it.
    //!
    //! Tests build the descriptions they check in `const` items, so each one
    //! also shows that `try_new` works at compile time.

    extern crate std;
    use super::*;
    use std::string::{String, ToString};
    use std::vec::Vec;

    const SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    const SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    type DescResult = Result<SoftwareDescription, SoftwareDescriptionError>;

    /// A clean library build.
    const LIBRARY: DescResult =
        SoftwareDescription::try_new("my-app", "1.2.3", SHA1, "false", None);
    const LIBRARY_RECORD: &str =
        "p=my-app;t=l;v=1.2.3;g=da39a3ee5e6b4b0d3255bfef95601890afd80709;d=0";

    /// A dirty binary build with a SHA-256 commit.
    const BINARY: DescResult =
        SoftwareDescription::try_new("my-app", "1.2.3-rc.1", SHA256, "true", Some("my-cli"));
    const BINARY_RECORD: &str = "p=my-app;t=b:my-cli;v=1.2.3-rc.1;\
        g=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855;d=1";

    /// `LIBRARY_RECORD` with the field for `key` given a different value.
    fn record_with(key: &str, value: &str) -> String {
        LIBRARY_RECORD
            .split(';')
            .map(|field| match field.split_once('=') {
                Some((k, _)) if k == key => std::format!("{key}={value}"),
                _ => field.into(),
            })
            .collect::<Vec<_>>()
            .join(";")
    }

    /// `LIBRARY_RECORD` without the field for `key`.
    fn record_without(key: &str) -> String {
        LIBRARY_RECORD
            .split(';')
            .filter(|field| !field.starts_with(&std::format!("{key}=")))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// The error for a value that's one byte over a field's capacity.
    fn too_long(cap: usize) -> StrToFixedFieldError {
        StrToFixedFieldError::FieldTooLong {
            source: CapacityError {
                len: cap + 1,
                capacity: cap,
            },
        }
    }

    mod construction {
        //! `try_new` must build a description holding exactly the given values.
        use super::*;

        /// Every field must hold the value it was given.
        #[test]
        fn fields_hold_given_values() {
            let d = BINARY.unwrap();
            assert_eq!(d.package.as_str(), "my-app");
            assert_eq!(d.version.as_str(), "1.2.3-rc.1");
            assert_eq!(d.git_sha.as_str(), SHA256);
            assert!(d.git_dirty);
        }

        /// No binary name must give a library description.
        #[test]
        fn no_binary_is_a_library() {
            assert_eq!(LIBRARY.unwrap().package_type, PackageType::Library);
        }

        /// A binary name must give a binary description with that name.
        #[test]
        fn binary_name_is_a_binary() {
            match BINARY.unwrap().package_type {
                PackageType::Binary(name) => assert_eq!(name.as_str(), "my-cli"),
                other => panic!("expected a binary, got {other:?}"),
            }
        }

        /// `"true"` and `"false"` must give a dirty and a clean description.
        #[test]
        fn dirty_flag() {
            const DIRTY: DescResult = SoftwareDescription::try_new("a", "1", SHA1, "true", None);
            const CLEAN: DescResult = SoftwareDescription::try_new("a", "1", SHA1, "false", None);
            assert!(DIRTY.unwrap().git_dirty);
            assert!(!CLEAN.unwrap().git_dirty);
        }

        /// Values exactly at each field's capacity must be accepted.
        #[test]
        fn values_at_capacity_are_accepted() {
            let package = "a".repeat(SoftwareDescription::PACKAGE_CAP);
            let version = "1".repeat(SoftwareDescription::VERSION_CAP);
            let binary = "b".repeat(SoftwareDescription::BINARY_CAP);
            assert!(
                SoftwareDescription::try_new(&package, &version, SHA1, "false", Some(&binary))
                    .is_ok()
            );
        }
    }

    mod validation {
        //! `try_new` must reject invalid input, saying which field was wrong
        //! and why.
        use super::*;
        use SoftwareDescriptionError::*;

        /// An empty or invalid package name, including one starting with a
        /// digit or `-`, must be rejected as an invalid package name.
        #[test]
        fn invalid_package_names_are_rejected() {
            let source = StrToFixedFieldError::InvalidPackageName;
            for bad in [
                "", "my app", "my.app", "my+app", "my;app", "my=app", "café", "2048", "-app",
            ] {
                assert_eq!(
                    SoftwareDescription::try_new(bad, "1", SHA1, "false", None),
                    Err(ConvertingPackageField { source }),
                    "package {bad:?}"
                );
            }
        }

        /// An empty or invalid binary name must be rejected as an invalid
        /// name.
        #[test]
        fn invalid_binary_names_are_rejected() {
            let source = StrToFixedFieldError::InvalidName;
            for bad in ["", "my app", "my.app", "my+app", "my;app", "my=app", "café"] {
                assert_eq!(
                    SoftwareDescription::try_new("a", "1", SHA1, "false", Some(bad)),
                    Err(ConvertingBinaryNameField { source }),
                    "binary {bad:?}"
                );
            }
        }

        /// A binary name starting with a digit or `-` must be accepted, as
        /// Cargo accepts it.
        #[test]
        fn binary_name_may_start_with_digit_or_hyphen() {
            for name in ["2048", "-app"] {
                let d = SoftwareDescription::try_new("a", "1", SHA1, "false", Some(name)).unwrap();
                assert_eq!(
                    d.package_type,
                    PackageType::Binary(FixedStr::try_from_str(name).unwrap())
                );
            }
        }

        /// An empty or invalid version must be rejected as an invalid version.
        #[test]
        fn invalid_versions_are_rejected() {
            let source = StrToFixedFieldError::InvalidVersion;
            for bad in ["", "1 2", "1_2", "1/2", "1;2", "1=2", "1.é"] {
                assert_eq!(
                    SoftwareDescription::try_new("a", bad, SHA1, "false", None),
                    Err(ConvertingVersionField { source }),
                    "version {bad:?}"
                );
            }
        }

        /// A value one byte over its field's capacity must be rejected as too
        /// long for that field, with the length and capacity.
        #[test]
        fn over_capacity_is_rejected_for_each_field() {
            let package = "x".repeat(SoftwareDescription::PACKAGE_CAP + 1);
            let version = "x".repeat(SoftwareDescription::VERSION_CAP + 1);
            let binary = "x".repeat(SoftwareDescription::BINARY_CAP + 1);
            assert_eq!(
                SoftwareDescription::try_new(&package, "1", SHA1, "false", None),
                Err(ConvertingPackageField {
                    source: too_long(SoftwareDescription::PACKAGE_CAP)
                })
            );
            assert_eq!(
                SoftwareDescription::try_new("a", &version, SHA1, "false", None),
                Err(ConvertingVersionField {
                    source: too_long(SoftwareDescription::VERSION_CAP)
                })
            );
            assert_eq!(
                SoftwareDescription::try_new("a", "1", SHA1, "false", Some(&binary)),
                Err(ConvertingBinaryNameField {
                    source: too_long(SoftwareDescription::BINARY_CAP)
                })
            );
        }

        /// A malformed git SHA must be rejected, with the `GitShaError` at the
        /// end of the chain.
        #[test]
        fn malformed_sha_is_rejected() {
            const SHORT: DescResult =
                SoftwareDescription::try_new("a", "1", "da39a3e", "false", None);
            assert_eq!(
                SHORT,
                Err(ConvertingGitShaField {
                    source: StrToFixedFieldError::InvalidGitSha {
                        source: GitShaError::BadLength { len: 7 }
                    }
                })
            );
        }

        /// The dirty flag must be exactly `"true"` or `"false"`.
        #[test]
        fn dirty_flag_must_be_true_or_false() {
            for bad in ["", "TRUE", "1", "0", "yes"] {
                assert_eq!(
                    SoftwareDescription::try_new("a", "1", SHA1, bad, None),
                    Err(ConvertingGitDirtyField {
                        source: StrToFixedFieldError::InvalidBool
                    }),
                    "{bad:?}"
                );
            }
        }

        /// vergen's placeholder must be rejected in either git field.
        #[test]
        fn vergen_placeholder_is_rejected() {
            const SHA: DescResult =
                SoftwareDescription::try_new("a", "1", VERGEN_PLACEHOLDER, "false", None);
            const DIRTY: DescResult =
                SoftwareDescription::try_new("a", "1", SHA1, VERGEN_PLACEHOLDER, None);
            assert_eq!(SHA, Err(VergenPlaceholder));
            assert_eq!(DIRTY, Err(VergenPlaceholder));
        }
    }

    mod encoding {
        //! `encode` must write `p=<package>;t=<l|b:binary>;v=<version>;g=<sha>;d=<0|1>`.
        use super::*;

        /// A library must be encoded with `t=l`.
        #[test]
        fn library_record() {
            assert_eq!(LIBRARY.unwrap().encode().as_str(), LIBRARY_RECORD);
        }

        /// A binary must be encoded with `t=b:` and its name.
        #[test]
        fn binary_record() {
            assert_eq!(BINARY.unwrap().encode().as_str(), BINARY_RECORD);
        }

        /// The dirty flag must be encoded as `d=1` or `d=0`.
        #[test]
        fn dirty_flag_is_one_or_zero() {
            assert!(BINARY.unwrap().encode().as_str().ends_with(";d=1"));
            assert!(LIBRARY.unwrap().encode().as_str().ends_with(";d=0"));
        }

        /// `Display` must write the encoded record.
        #[test]
        fn display_is_the_record() {
            let d = BINARY.unwrap();
            assert_eq!(d.to_string(), d.encode().as_str());
        }
    }

    mod record_size {
        //! Every description must encode to at most `DNS_STRING_MAX` bytes.
        use super::*;

        /// `DNS_STRING_MAX` must be the maximum length of a DNS character-string.
        #[test]
        fn dns_string_max_is_255() {
            assert_eq!(DNS_STRING_MAX, 255);
        }

        /// The largest possible description must encode without loss and fit.
        /// (A compile-time check in this module proves the same for the
        /// capacities themselves.)
        #[test]
        fn largest_description_fits() {
            let package = "a".repeat(SoftwareDescription::PACKAGE_CAP);
            let version = "1".repeat(SoftwareDescription::VERSION_CAP);
            let binary = "b".repeat(SoftwareDescription::BINARY_CAP);
            let d = SoftwareDescription::try_new(&package, &version, SHA256, "true", Some(&binary))
                .unwrap();
            let record = d.try_encode().unwrap();
            assert_eq!(record, d.encode());
            assert_eq!(record.len(), 241);
            assert!(record.len() <= DNS_STRING_MAX);
        }
    }

    mod parsing {
        //! `parse` must recover exactly the description that was encoded.
        use super::*;

        /// Encoding then parsing must give back the same description.
        #[test]
        fn round_trips() {
            let package = "a".repeat(SoftwareDescription::PACKAGE_CAP);
            let version = "1".repeat(SoftwareDescription::VERSION_CAP);
            let binary = "b".repeat(SoftwareDescription::BINARY_CAP);
            let largest =
                SoftwareDescription::try_new(&package, &version, SHA256, "true", Some(&binary))
                    .unwrap();
            for d in [LIBRARY.unwrap(), BINARY.unwrap(), largest] {
                assert_eq!(SoftwareDescription::parse(d.encode().as_str()), Ok(d));
            }
        }

        /// `parse_bytes` must give the same result as `parse`.
        #[test]
        fn parse_bytes_matches_parse() {
            assert_eq!(
                SoftwareDescription::parse_bytes(BINARY_RECORD.as_bytes()),
                SoftwareDescription::parse(BINARY_RECORD)
            );
        }

        /// `FromStr` must give the same result as `parse`.
        #[test]
        fn from_str_matches_parse() {
            assert_eq!(
                BINARY_RECORD.parse::<SoftwareDescription>(),
                SoftwareDescription::parse(BINARY_RECORD)
            );
        }

        /// Fields after the dirty flag must be ignored, so a future version can
        /// append fields.
        #[test]
        fn fields_after_dirty_flag_are_ignored() {
            for extra in ["x=1", "x=1;y=2", "d=1", ""] {
                let record = std::format!("{LIBRARY_RECORD};{extra}");
                assert_eq!(
                    SoftwareDescription::parse(&record),
                    Ok(LIBRARY.unwrap()),
                    "{extra:?}"
                );
            }
        }

        /// A single trailing `;` must be accepted.
        #[test]
        fn trailing_separator_is_accepted() {
            let record = std::format!("{LIBRARY_RECORD};");
            assert_eq!(SoftwareDescription::parse(&record), Ok(LIBRARY.unwrap()));
        }
    }

    mod parse_errors {
        //! `parse` must reject malformed records, saying what was wrong.
        use super::*;
        use SoftwareDescriptionError::*;

        /// A record longer than `DNS_STRING_MAX` must be rejected with its length.
        #[test]
        fn too_long_is_rejected() {
            let record = "x".repeat(DNS_STRING_MAX + 1);
            assert_eq!(
                SoftwareDescription::parse(&record),
                Err(TooLong { len: 256 })
            );
            assert_eq!(
                SoftwareDescription::parse_bytes(record.as_bytes()),
                Err(TooLong { len: 256 })
            );
        }

        /// A record of exactly `DNS_STRING_MAX` bytes must not be rejected as
        /// too long.
        #[test]
        fn max_length_is_not_too_long() {
            let record = "x".repeat(DNS_STRING_MAX);
            assert_eq!(
                SoftwareDescription::parse(&record),
                Err(MissingPackageField)
            );
        }

        /// Raw bytes that aren't UTF-8 must be rejected.
        #[test]
        fn non_utf8_is_rejected() {
            assert!(matches!(
                SoftwareDescription::parse_bytes(b"p=\xff;v=1"),
                Err(NotUtf8 { .. })
            ));
        }

        /// A missing field must be rejected, naming the field.
        #[test]
        fn missing_field_is_named() {
            for (key, error) in [
                ("p", MissingPackageField),
                ("t", MissingPackageTypeField),
                ("v", MissingVersionField),
                ("g", MissingGitShaField),
                ("d", MissingGitDirtyField),
            ] {
                assert_eq!(
                    SoftwareDescription::parse(&record_without(key)),
                    Err(error),
                    "{key}"
                );
            }
        }

        /// An empty record must be rejected as missing its first field.
        #[test]
        fn empty_record_is_missing_package() {
            assert_eq!(SoftwareDescription::parse(""), Err(MissingPackageField));
        }

        /// Fields out of order must be rejected.
        #[test]
        fn fields_out_of_order_are_rejected() {
            let swapped = LIBRARY_RECORD.replacen("t=l;v=1.2.3", "v=1.2.3;t=l", 1);
            assert_eq!(
                SoftwareDescription::parse(&swapped),
                Err(MissingPackageTypeField)
            );
        }

        /// An invalid package name, version or binary name must be rejected
        /// as invalid for that field.
        #[test]
        fn invalid_values_are_rejected() {
            let package = StrToFixedFieldError::InvalidPackageName;
            let name = StrToFixedFieldError::InvalidName;
            let version = StrToFixedFieldError::InvalidVersion;
            assert_eq!(
                SoftwareDescription::parse(&record_with("p", "2048")),
                Err(ConvertingPackageField { source: package })
            );
            assert_eq!(
                SoftwareDescription::parse(&record_with("v", "")),
                Err(ConvertingVersionField { source: version })
            );
            assert_eq!(
                SoftwareDescription::parse(&record_with("t", "b:")),
                Err(ConvertingBinaryNameField { source: name })
            );
        }

        /// A value over its field's capacity must be rejected as too long.
        #[test]
        fn over_capacity_is_rejected() {
            let package = "a".repeat(SoftwareDescription::PACKAGE_CAP + 1);
            assert_eq!(
                SoftwareDescription::parse(&record_with("p", &package)),
                Err(ConvertingPackageField {
                    source: too_long(SoftwareDescription::PACKAGE_CAP)
                })
            );
        }

        /// A malformed git SHA must be rejected, with the `GitShaError` at the
        /// end of the chain.
        #[test]
        fn malformed_sha_is_rejected() {
            assert_eq!(
                SoftwareDescription::parse(&record_with("g", "abc")),
                Err(ConvertingGitShaField {
                    source: StrToFixedFieldError::InvalidGitSha {
                        source: GitShaError::BadLength { len: 3 }
                    }
                })
            );
        }

        /// The dirty flag must be exactly `0` or `1`.
        #[test]
        fn dirty_flag_must_be_zero_or_one() {
            for bad in ["", "2", "true", "yes"] {
                assert_eq!(
                    SoftwareDescription::parse(&record_with("d", bad)),
                    Err(ConvertingGitDirtyField {
                        source: StrToFixedFieldError::InvalidBool
                    }),
                    "{bad:?}"
                );
            }
        }

        /// The package type must be `l` or `b:<name>`.
        #[test]
        fn package_type_must_be_l_or_b() {
            for bad in ["", "x", "b", "lib", "L", "c:name"] {
                assert_eq!(
                    SoftwareDescription::parse(&record_with("t", bad)),
                    Err(InvalidPackageType),
                    "{bad:?}"
                );
            }
        }
    }

    mod errors {
        //! Every error must explain itself, both as a compile-time report and
        //! at runtime, through its whole source chain.
        use super::*;
        use SoftwareDescriptionError::*;
        use core::error::Error as _;

        fn every_error() -> [SoftwareDescriptionError; 20] {
            // `black_box` stops the compiler warning that this always fails.
            let utf8 = core::str::from_utf8(core::hint::black_box(&[0xff])).unwrap_err();
            let package = StrToFixedFieldError::InvalidPackageName;
            let name = StrToFixedFieldError::InvalidName;
            let version = StrToFixedFieldError::InvalidVersion;
            let long = too_long(64);
            let sha = StrToFixedFieldError::InvalidGitSha {
                source: GitShaError::NotHex { index: 3 },
            };
            [
                VergenPlaceholder,
                TooLong { len: 300 },
                NotUtf8 { source: utf8 },
                MissingPackageField,
                MissingPackageTypeField,
                MissingVersionField,
                MissingGitShaField,
                MissingGitDirtyField,
                InvalidPackageType,
                ConvertingPackageField { source: package },
                ConvertingPackageField { source: long },
                ConvertingBinaryNameField { source: name },
                ConvertingBinaryNameField { source: long },
                ConvertingVersionField { source: version },
                ConvertingVersionField { source: long },
                ConvertingGitShaField { source: sha },
                ConvertingGitShaField {
                    source: StrToFixedFieldError::InvalidGitSha {
                        source: GitShaError::BadLength { len: 3 },
                    },
                },
                ConvertingGitDirtyField {
                    source: StrToFixedFieldError::InvalidBool,
                },
                ConvertingGitShaField { source: name },
                ConvertingPackageField { source: sha },
            ]
        }

        /// The `const_message` of an error in a source chain, if it's one of
        /// this crate's error types.
        fn const_message_of(e: &(dyn core::error::Error + 'static)) -> Option<&'static str> {
            if let Some(e) = e.downcast_ref::<SoftwareDescriptionError>() {
                Some(e.const_message())
            } else if let Some(e) = e.downcast_ref::<StrToFixedFieldError>() {
                Some(e.const_message())
            } else if let Some(e) = e.downcast_ref::<CapacityError>() {
                Some(e.const_message())
            } else if let Some(e) = e.downcast_ref::<GitShaError>() {
                Some(e.const_message())
            } else {
                None
            }
        }

        /// Each error's `Display` must start with its `const_message`.
        #[test]
        fn display_starts_with_const_message() {
            for e in every_error() {
                assert!(e.to_string().starts_with(e.const_message()), "{e:?}");
            }
            for e in [
                StrToFixedFieldError::InvalidPackageName,
                StrToFixedFieldError::InvalidName,
                StrToFixedFieldError::InvalidVersion,
                too_long(64),
                StrToFixedFieldError::InvalidGitSha {
                    source: GitShaError::NotHex { index: 3 },
                },
                StrToFixedFieldError::InvalidBool,
            ] {
                assert!(e.to_string().starts_with(e.const_message()), "{e:?}");
            }
        }

        /// Each field's conversion error must name the field.
        #[test]
        fn conversion_messages_name_the_field() {
            let source = StrToFixedFieldError::InvalidName;
            for (e, field) in [
                (ConvertingPackageField { source }, "package name"),
                (ConvertingBinaryNameField { source }, "binary name"),
                (ConvertingVersionField { source }, "package version"),
                (ConvertingGitShaField { source }, "git SHA"),
                (ConvertingGitDirtyField { source }, "git dirty flag"),
            ] {
                assert!(e.const_message().contains(field), "{e:?}");
            }
        }

        /// A conversion error's source chain must lead through the
        /// `StrToFixedFieldError` to the underlying error.
        #[test]
        fn source_chain_reaches_the_underlying_error() {
            let cap = CapacityError {
                len: 65,
                capacity: 64,
            };
            let e = ConvertingPackageField {
                source: StrToFixedFieldError::FieldTooLong { source: cap },
            };
            let field = e.source().unwrap();
            assert_eq!(field.to_string(), too_long(64).to_string());
            let cause = field.source().unwrap();
            assert_eq!(cause.to_string(), cap.to_string());
            assert!(cause.source().is_none());
            assert!(MissingPackageField.source().is_none());
        }

        /// `const_report` must join the `const_message`s of the whole source
        /// chain.
        #[test]
        fn const_report_joins_the_whole_chain() {
            const REPORT: FixedStr<{ SoftwareDescriptionError::REPORT_CAP }> =
                ConvertingGitShaField {
                    source: StrToFixedFieldError::InvalidGitSha {
                        source: GitShaError::NotHex { index: 3 },
                    },
                }
                .const_report();
            assert_eq!(
                REPORT.as_str(),
                "invalid git SHA: not a full hex git SHA: git SHA contains a non-hex character"
            );
        }

        /// `const_report` for an error with no source must be its `const_message`.
        #[test]
        fn const_report_without_a_source_is_the_message() {
            const REPORT: FixedStr<{ SoftwareDescriptionError::REPORT_CAP }> =
                MissingVersionField.const_report();
            assert_eq!(REPORT.as_str(), MissingVersionField.const_message());
        }

        /// Every error's `const_report` must match its runtime source chain,
        /// and fit in `REPORT_CAP`, so it never falls back to an empty report.
        #[test]
        fn every_report_matches_the_source_chain() {
            for e in every_error() {
                let mut expected = Vec::new();
                let mut next: Option<&(dyn core::error::Error + 'static)> = Some(&e);
                while let Some(err) = next {
                    match const_message_of(err) {
                        Some(message) => expected.push(message),
                        None => break, // e.g. `Utf8Error`, which has no const message
                    }
                    next = err.source();
                }
                assert_eq!(e.const_report().as_str(), expected.join(": "), "{e:?}");
            }
        }
    }
}
