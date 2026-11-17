# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.9.0] - 2026-11-17

First version of `chiffchaff`: a small `no_std` crate that lets a piece of
software say exactly which build it is.

### Added

- A description of the running build: package name, whether it's a library or
  a named binary, version, git commit and whether the working tree was dirty.
- The `get_crate_version!` macro, which builds that description for the
  current crate at compile time. Invalid or missing information is a compile
  error with a readable explanation, not a runtime surprise.
- A compact text record of the description, always short enough to publish in
  a single DNS TXT string (255 bytes), for example
  `p=my-app;t=b:my-cli;v=1.2.3;g=<commit>;d=0`.
- Parsing of that record back into a description, with errors that say which
  field was wrong and why. Fields added after the dirty flag by future
  versions are ignored, so older readers keep working.
- An optional `build` feature with a build-script helper that records the git
  commit, taken from `.cargo_vcs_info.json` for packaged crates or from git
  otherwise. A build with no known commit fails rather than producing an
  unverifiable version.
- Validation that names and versions only contain the characters Cargo
  allows, so every record can be read back unambiguously.
- Support for both SHA-1 and SHA-256 git repositories.
- Works without the standard library or a heap; everything is fixed-size.

<!-- next-url -->
[Unreleased]: https://github.com/SmartAcoustics/chiffchaff/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/SmartAcoustics/chiffchaff/commits/main
