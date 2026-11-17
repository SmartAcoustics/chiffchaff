// This crate was built with extensive assistance from the Claude AI chat interface.
// It was audited by working through each test to make every test fail, then the
// functionality was brought back in to action, in some cases with extensive manual
// rewrites to improve the design. The full crate has been carefully and properly
// modified and reviewed to pass internal design standards.

//! Identifies exactly which build of a piece of software is running, in a
//! record small enough for a single DNS TXT string (at most
//! [`DNS_STRING_MAX`] = 255 bytes).
//!
//! A [`SoftwareDescription`] holds a crate's package name, its
//! [`PackageType`] (a library, or a binary with its name), its version, the
//! git commit it was built from, and whether the working tree was dirty. It's
//! built at compile time, encoded into a [`Record`] with
//! [`SoftwareDescription::encode`], and parsed back with
//! [`SoftwareDescription::parse`] by whatever checks the running version.
//!
//! # Usage
//!
//! Depend on the crate twice in `Cargo.toml`: normally for the code, and with
//! the `build` feature for the build script:
//!
//! ```toml
//! [dependencies]
//! chiffchaff = "0.9"
//!
//! [build-dependencies]
//! chiffchaff = { version = "0.9", features = ["build"] }
//! ```
//!
//! Call `chiffchaff::build::emit` from `build.rs`. It finds the git commit
//! and dirty flag and passes them to the compiler:
//!
//! ```ignore
//! // build.rs
//! fn main() -> chiffchaff::build::Report {
//!     chiffchaff::build::Report::capture(chiffchaff::build::emit)
//! }
//! ```
//!
//! Then build the description with [`get_crate_version!`], in the crate that
//! is shipped (normally the binary):
//!
//! ```ignore
//! use chiffchaff::{SoftwareDescription, Record}
//!
//! struct App;
//!
//! const DESCRIPTION: SoftwareDescription = chiffchaff::get_crate_version!();
//!
//! fn main() {
//!     // The encoded record, computed at compile time, e.g.
//!     // p=my-app;t=b:my-app;v=1.2.3;g=da39a3ee…;d=0
//!     const RECORD: Record = DESCRIPTION.encode();
//!     println!("{RECORD}");
//! }
//! ```
//!
//! To check a running build, parse its TXT record and compare it with the
//! description you expect:
//!
//! ```
//! use chiffchaff::SoftwareDescription;
//!
//! # let expected = SoftwareDescription::try_new(
//! #     "my-app", "1.2.3", "da39a3ee5e6b4b0d3255bfef95601890afd80709", "false", Some("my-app"),
//! # ).unwrap();
//! # let record = expected.encode();
//! # let txt_bytes = record.as_bytes();
//! // `txt_bytes` is the TXT record's character-string, fetched from DNS.
//! let running = SoftwareDescription::parse_bytes(txt_bytes)?;
//! assert_eq!(running, expected);
//! # Ok::<(), chiffchaff::SoftwareDescriptionError>(())
//! ```
//!
//! # Record format
//!
//! `p=<package>;t=<l|b:binary>;v=<version>;g=<git SHA>;d=<0|1>`
//!
//! | Key | Field | Value |
//! |---|---|---|
//! | `p` | package name | up to [`SoftwareDescription::PACKAGE_CAP`] (64) bytes |
//! | `t` | package type | `l` for a library, `b:<name>` for a binary; the name is up to [`SoftwareDescription::BINARY_CAP`] (32) bytes |
//! | `v` | version | up to [`SoftwareDescription::VERSION_CAP`] (64) bytes |
//! | `g` | git commit | 40 (SHA-1) or 64 (SHA-256) lowercase hex digits |
//! | `d` | dirty | `1` if tracked files had uncommitted changes, otherwise `0` |
//!
//! The parser accepts the fields only in that order and requires all five. It
//! ignores later unknown keys, so fields can be added later
//! without breaking older parsers.
//!
//! # Where the git commit comes from
//!
//! `build::emit` reads `.cargo_vcs_info.json` if the crate has one (it was
//! packaged by `cargo package` or `cargo publish`, for example it came from a
//! registry or was vendored). Otherwise the crate must be in a git checkout.
//! If neither is available, the build fails: source that isn't tracked by git
//! can't be built.
//!
//! Crates published from SHA-256 git repositories currently can't be built
//! from the registry: Cargo (as of 1.97) doesn't recognise SHA-256
//! repositories, and packages them without a `.cargo_vcs_info.json`.
//!
//! # Package type
//!
//! [`get_crate_version!`] gives [`PackageType::Binary`] when it's expanded in a
//! binary target (Cargo sets `CARGO_BIN_NAME`) and [`PackageType::Library`]
//! otherwise. A description built in a library crate and used from a binary is
//! therefore a `Library` description, so build it in the binary.
//!
//! # Guarantees
//!
//! - The crate is `no_std`. Only the `build` feature, which is meant for build
//!   scripts, uses `std`.
//! - A `SoftwareDescription` can be built in a const context, so invalid input
//!   (a malformed git SHA, an over-long package name, and so on) is a compile
//!   error, not a runtime panic.
//! - No public function panics. Fallible operations return `Result` or
//!   `Option`; the only panic is inside [`get_crate_version!`]'s `const`, where
//!   it turns invalid input into a compile error.
//! - Every `SoftwareDescription` fits in a `Record`. A compile-time check
//!   encodes the largest possible description, so if the field capacities are
//!   changed so that a record could overflow, this crate stops compiling.
//! - A `Record` represented as a `str` will always fit within a 255 byte
//!   DNS record.
#![no_std]

#[cfg(feature = "build")]
extern crate std;

#[cfg(feature = "build")]
pub mod build;

mod const_str;
mod fixed_str;
mod git_sha;
mod software_description;

pub use fixed_str::{CapacityError, FixedStr};
pub use git_sha::{GitSha, GitShaError, HexStr};
pub use software_description::{
    DNS_STRING_MAX, PackageType, Record, SoftwareDescription, SoftwareDescriptionError,
    StrToFixedFieldError, VERGEN_PLACEHOLDER,
};

/// Builds the [`SoftwareDescription`] of the crate that invokes the macro,
/// from `CARGO_PKG_NAME`, `CARGO_PKG_VERSION`, `CARGO_BIN_NAME`,
/// `VERGEN_GIT_SHA` and `VERGEN_GIT_DIRTY`.
///
/// The invoking crate's `build.rs` must call `chiffchaff::build::emit`
/// (feature `build`), which sets the two git variables. See the
/// [crate documentation](crate#usage) for the full setup, and
/// [Package type](crate#package-type) for how the package type is chosen.
///
/// The value is always computed in a `const`, so invalid input is a compile
/// error even if the macro is used inside a function body.
#[macro_export]
macro_rules! get_crate_version {
    () => {{
        const DESCRIPTION: $crate::SoftwareDescription = match $crate::SoftwareDescription::try_new(
            ::core::env!("CARGO_PKG_NAME"),
            ::core::env!("CARGO_PKG_VERSION"),
            ::core::env!(
                "VERGEN_GIT_SHA",
                "VERGEN_GIT_SHA is not set: call chiffchaff::build::emit() from this crate's build.rs"
            ),
            ::core::env!(
                "VERGEN_GIT_DIRTY",
                "VERGEN_GIT_DIRTY is not set: call chiffchaff::build::emit() from this crate's build.rs"
            ),
            ::core::option_env!("CARGO_BIN_NAME"),
        ) {
            ::core::result::Result::Ok(description) => description,
            // Evaluated at compile time: this is a compile error, not a
            // runtime panic.
            ::core::result::Result::Err(e) => ::core::panic!("{}", e.const_report().as_str()),
        };
        DESCRIPTION
    }};
}
