# chiffchaff

[![CI](https://github.com/SmartAcoustics/chiffchaff/actions/workflows/ci.yml/badge.svg)](https://github.com/SmartAcoustics/chiffchaff/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/chiffchaff.svg)](https://crates.io/crates/chiffchaff)
[![docs.rs](https://img.shields.io/docsrs/chiffchaff)](https://docs.rs/chiffchaff)

A small `no_std` Rust crate that lets a piece of software say exactly which
build it is: package name, whether it's a library or a named binary, version,
git commit and whether the working tree was dirty.

The description is built at compile time and encodes to a short text record
that always fits in a single DNS TXT string (255 bytes):

```text
p=my-app;t=b:my-cli;v=1.2.3;g=da39a3ee5e6b4b0d3255bfef95601890afd80709;d=0
```

Publish that record wherever it's useful (DNS, a status endpoint, a log
line) and anything that reads it can tell precisely what is running.

## Why

- **Always tied to a commit.** A build with no known git commit fails, so
  every running version can be traced back to its source.
- **Checked at compile time.** Missing or invalid information is a compile
  error with a readable explanation, not a surprise in production.
- **Small and predictable.** No standard library, no heap, fixed-size types,
  and no public function panics.

## Usage

Add the crate twice in `Cargo.toml`: once for your code, and once with the
`build` feature for your build script.

```toml
[dependencies]
chiffchaff = "0.9"

[build-dependencies]
chiffchaff = { version = "0.9", features = ["build"] }
```

In `build.rs`, let it find the git commit and pass it to the compiler:

```rust
fn main() -> chiffchaff::build::Report {
    chiffchaff::build::Report::capture(chiffchaff::build::emit)
}
```

Then build the description in the crate you ship (normally the binary):

```rust
use chiffchaff::{Record, SoftwareDescription};

const VERSION: SoftwareDescription = chiffchaff::get_crate_version!();
const RECORD: Record = VERSION.encode();

fn main() {
    println!("{RECORD}");
}
```

To check what's running, parse its record and compare:

```rust
let running = chiffchaff::SoftwareDescription::parse(record)?;
assert_eq!(running, expected);
```

## Record format

`p=<package>;t=<l|b:binary>;v=<version>;g=<git commit>;d=<0|1>`

| Key | Meaning |
|-----|---------|
| `p` | Package name, up to 64 bytes |
| `t` | `l` for a library, or `b:<name>` for a binary (name up to 32 bytes) |
| `v` | Version, up to 64 bytes |
| `g` | Full git commit hash: SHA-1 or SHA-256 |
| `d` | `1` if tracked files had uncommitted changes, otherwise `0` |

The fields always appear in this order. Anything after `d=` is ignored, so
later versions can add fields without breaking older readers.

## Where the git commit comes from

For a crate installed from a registry or vendored, the commit is read from the
`.cargo_vcs_info.json` that `cargo package` writes. Otherwise the crate must be
built from a git checkout. If neither is available, the build fails.

Cargo doesn't currently record the commit when packaging from a SHA-256 git
repository, so such crates can be built from a checkout but not from a
registry.

## Library or binary

`get_crate_version!` records a binary, with its name, when it's used in a
binary target, and a library otherwise. Use it in the binary you ship, so the
record names that binary.

## Minimum supported Rust version

Rust 1.96.

## Changes

See [CHANGELOG.md](CHANGELOG.md).
