//! Build-script support: sets `VERGEN_GIT_SHA` and `VERGEN_GIT_DIRTY` for
//! [`get_crate_version!`](crate::get_crate_version).
//!
//! Add `magpie-version` as a build-dependency with the `build` feature, then:
//!
//! ```ignore
//! // build.rs
//! fn main() -> magpie_version::build::Report {
//!     magpie_version::build::Report::capture(magpie_version::build::emit)
//! }
//! ```
//!
//! Returning a [`Report`] prints the error and its causes readably if the
//! build script fails.

use std::boxed::Box;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::string::{String, ToString};
use std::{env, fmt, fs, io};

use serde_json::Value;
use snafu::{OptionExt, ResultExt, Snafu};
use vergen_gitcl::{Emitter, Gitcl};

/// Why the git commit for the crate being built couldn't be determined.
#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("CARGO_MANIFEST_DIR is not set; emit() must be called from a build script"))]
    NoManifestDir,

    #[snafu(display("couldn't read {}", path.display()))]
    ReadVcsInfo { path: PathBuf, source: io::Error },

    #[snafu(display("couldn't parse {}", path.display()))]
    ParseVcsInfo {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[snafu(display("{} has no \"{field}\"", path.display()))]
    MissingVcsField { path: PathBuf, field: &'static str },

    #[snafu(display("{} has an invalid \"{field}\"", path.display()))]
    InvalidVcsField { path: PathBuf, field: &'static str },

    #[snafu(display(
        "couldn't read git info: no .cargo_vcs_info.json and not a usable git checkout"
    ))]
    Git {
        #[snafu(source(from(anyhow::Error, Into::into)))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("couldn't write cargo instructions to stdout"))]
    WriteInstruction { source: io::Error },
}

/// Lets a build script's `main` print a [`BuildError`] and its causes readably.
pub type Report = snafu::Report<BuildError>;

/// Emits the git commit and dirty flag for the crate being built, and fails
/// if neither source is available.
///
/// - If the crate has a `.cargo_vcs_info.json` (it was packaged by
///   `cargo package`/`cargo publish`, e.g. it came from a registry), that file
///   is used. It's checked first so a vendored crate inside another repository
///   reports its own commit, not that repository's.
/// - Otherwise the crate must be in a git checkout, queried via vergen.
pub fn emit() -> Result<(), BuildError> {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").context(NoManifestDirSnafu)?);
    let vcs_info = manifest_dir.join(".cargo_vcs_info.json");
    if vcs_info.exists() {
        from_vcs_info(&vcs_info)
    } else {
        from_git()
    }
}

/// The commit and dirty flag recorded in a `.cargo_vcs_info.json`.
#[derive(Debug, PartialEq, Eq)]
struct VcsInfo {
    sha: String,
    dirty: bool,
}

fn from_vcs_info(path: &Path) -> Result<(), BuildError> {
    let VcsInfo { sha, dirty } = load_vcs_info(path)?;
    cargo(format_args!("rerun-if-changed={}", path.display()))?;
    cargo(format_args!("rustc-env=VERGEN_GIT_SHA={sha}"))?;
    cargo(format_args!("rustc-env=VERGEN_GIT_DIRTY={dirty}"))
}

/// Reads and parses the `.cargo_vcs_info.json` at `path`.
fn load_vcs_info(path: &Path) -> Result<VcsInfo, BuildError> {
    let contents = fs::read_to_string(path).context(ReadVcsInfoSnafu { path })?;
    parse_vcs_info(path, &contents)
}

/// Parses `{"git": {"sha1": "...", "dirty": true}}`. Cargo omits `dirty` when
/// the tree was clean. `path` is only used in error messages.
fn parse_vcs_info(path: &Path, contents: &str) -> Result<VcsInfo, BuildError> {
    let json: Value = serde_json::from_str(contents).context(ParseVcsInfoSnafu { path })?;

    // Indexing a `Value` never panics: a missing key (or a missing `git`
    // section) gives `Value::Null`.
    let sha = match &json["git"]["sha1"] {
        Value::String(sha) => sha.clone(),
        Value::Null => {
            return MissingVcsFieldSnafu {
                path,
                field: "git.sha1",
            }
            .fail();
        }
        _ => {
            return InvalidVcsFieldSnafu {
                path,
                field: "git.sha1",
            }
            .fail();
        }
    };

    let dirty = match &json["git"]["dirty"] {
        Value::Null => false, // absent: Cargo omits it when the tree was clean
        Value::Bool(dirty) => *dirty,
        _ => {
            return InvalidVcsFieldSnafu {
                path,
                field: "git.dirty",
            }
            .fail();
        }
    };

    Ok(VcsInfo { sha, dirty })
}

fn from_git() -> Result<(), BuildError> {
    let git = Gitcl::builder()
        .sha(false) // full SHA: `false` turns off `--short`
        .dirty(false) // `true` would also count untracked files as dirty
        .build();
    Emitter::default()
        .fail_on_error()
        .add_instructions(&git)
        .context(GitSnafu)?
        .emit()
        .context(GitSnafu)?;

    // vergen only reruns this script when the commit changes (.git/HEAD and
    // the branch ref), so editing files wouldn't update the dirty flag. Also
    // rerun when this crate's files or git's index change. Directories are
    // scanned recursively.
    cargo(format_args!("rerun-if-changed=src"))?;
    cargo(format_args!("rerun-if-changed=Cargo.toml"))?;
    if let Some(index) = git_index_path() {
        cargo(format_args!("rerun-if-changed={index}"))?;
    }
    Ok(())
}

/// Writes a `cargo:` instruction. Unlike `println!`, fails rather than
/// panicking if stdout can't be written.
fn cargo(instruction: fmt::Arguments<'_>) -> Result<(), BuildError> {
    writeln!(io::stdout(), "cargo:{instruction}").context(WriteInstructionSnafu)
}

/// Absolute path to git's index file (changes on `git add`, `git commit`, etc.).
fn git_index_path() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-path", "index"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

#[cfg(test)]
mod tests {
    //! The `build` feature must give a build script the git commit and dirty
    //! flag of the crate being built, reading `.cargo_vcs_info.json` for a
    //! packaged crate, and must fail if the information is missing or
    //! malformed.
    //!
    //! Choosing between `.cargo_vcs_info.json` and git, and running vergen,
    //! need a real build script, so they aren't tested here.

    use super::*;
    use std::format;

    const SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

    fn parse(contents: &str) -> Result<VcsInfo, BuildError> {
        // First argument is just used for populating error messages, so need not
        // be anything meaningful wrt to the function of tests beyond checking errors
        // are correct.
        parse_vcs_info(Path::new("pkg/.cargo_vcs_info.json"), contents)
    }

    fn info(sha: &str, dirty: bool) -> VcsInfo {
        VcsInfo {
            sha: sha.into(),
            dirty,
        }
    }

    mod vcs_info {
        //! A `.cargo_vcs_info.json` must give the commit and dirty flag that
        //! Cargo recorded in it.
        use super::*;

        /// The commit must be read from `git.sha1`.
        #[test]
        fn sha_is_read() {
            let json = format!(r#"{{"git": {{"sha1": "{SHA1}", "dirty": false}}}}"#);
            assert_eq!(parse(&json).unwrap(), info(SHA1, false));
        }

        /// `"dirty": true` must give a dirty build.
        #[test]
        fn dirty_true_is_dirty() {
            let json = format!(r#"{{"git": {{"sha1": "{SHA1}", "dirty": true}}}}"#);
            assert_eq!(parse(&json).unwrap(), info(SHA1, true));
        }

        /// A missing `dirty` must give a clean build, since Cargo omits it
        /// when the tree was clean.
        #[test]
        fn absent_dirty_is_clean() {
            let json = format!(r#"{{"git": {{"sha1": "{SHA1}"}}}}"#);
            assert_eq!(parse(&json).unwrap(), info(SHA1, false));
        }

        /// The commit must be passed through unchanged; validating it is
        /// `get_crate_version!`'s job, and a SHA-256 hash must not be rejected
        /// for being in a field called `sha1`.
        #[test]
        fn sha_is_passed_through_unchanged() {
            let sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
            let json = format!(r#"{{"git": {{"sha1": "{sha256}"}}}}"#);
            assert_eq!(parse(&json).unwrap().sha, sha256);
        }

        /// Other keys, such as Cargo's `path_in_vcs`, must be ignored.
        #[test]
        fn other_keys_are_ignored() {
            let json = format!(
                r#"{{"git": {{"sha1": "{SHA1}", "extra": 1}}, "path_in_vcs": "crates/app"}}"#
            );
            assert_eq!(parse(&json).unwrap(), info(SHA1, false));
        }

        /// A `.cargo_vcs_info.json` on disk must be read and parsed.
        #[test]
        fn file_is_loaded() {
            let dir =
                std::env::temp_dir().join(format!("magpie-version-test-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            // Nothing in here panics, so the cleanup below always runs.
            let loaded = (|| -> Result<VcsInfo, Box<dyn std::error::Error>> {
                let path = dir.join(".cargo_vcs_info.json");
                fs::write(
                    &path,
                    format!(r#"{{"git": {{"sha1": "{SHA1}", "dirty": true}}}}"#),
                )?;
                Ok(load_vcs_info(&path)?)
            })();
            fs::remove_dir_all(&dir).unwrap();
            assert_eq!(loaded.unwrap(), info(SHA1, true));
        }
    }

    mod vcs_info_errors {
        //! A missing or malformed `.cargo_vcs_info.json` must fail, saying
        //! which file and which field was wrong.
        use super::*;

        /// A missing `git.sha1` must fail as a missing field.
        #[test]
        fn missing_sha_is_missing() {
            assert!(matches!(
                parse(r#"{"git": {"dirty": true}}"#),
                Err(BuildError::MissingVcsField {
                    field: "git.sha1",
                    ..
                })
            ));
        }

        /// A missing `git` section must fail as a missing `git.sha1`.
        #[test]
        fn missing_git_section_is_missing_sha() {
            assert!(matches!(
                parse(r#"{"path_in_vcs": ""}"#),
                Err(BuildError::MissingVcsField {
                    field: "git.sha1",
                    ..
                })
            ));
        }

        /// A `git.sha1` that isn't a string must fail as an invalid field.
        #[test]
        fn non_string_sha_is_invalid() {
            assert!(matches!(
                parse(r#"{"git": {"sha1": 123}}"#),
                Err(BuildError::InvalidVcsField {
                    field: "git.sha1",
                    ..
                })
            ));
        }

        /// A `git.dirty` that isn't a boolean must fail as an invalid field,
        /// not be treated as clean.
        #[test]
        fn non_bool_dirty_is_invalid() {
            for dirty in [r#""true""#, "1", "[]"] {
                let json = format!(r#"{{"git": {{"sha1": "{SHA1}", "dirty": {dirty}}}}}"#);
                assert!(
                    matches!(
                        parse(&json),
                        Err(BuildError::InvalidVcsField {
                            field: "git.dirty",
                            ..
                        })
                    ),
                    "{dirty}"
                );
            }
        }

        /// A file that isn't valid JSON must fail as a parse error.
        #[test]
        fn invalid_json_is_a_parse_error() {
            assert!(matches!(
                parse("{not json"),
                Err(BuildError::ParseVcsInfo { .. })
            ));
        }

        /// A file that can't be read must fail as a read error.
        #[test]
        fn unreadable_file_is_a_read_error() {
            let path = Path::new("/nonexistent/magpie-version/.cargo_vcs_info.json");
            assert!(matches!(
                load_vcs_info(path),
                Err(BuildError::ReadVcsInfo { .. })
            ));
        }

        /// Error messages must name the file and the field.
        #[test]
        fn messages_name_the_file_and_field() {
            let message = parse(r#"{"git": {"sha1": 123}}"#).unwrap_err().to_string();
            assert_eq!(
                message,
                r#"pkg/.cargo_vcs_info.json has an invalid "git.sha1""#
            );
        }
    }
}
