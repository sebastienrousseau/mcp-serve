# Migrating the servers to mcp-serve

Three servers carry their own copy of the transport module: about 1,300
lines each of command line, stdio, streamable HTTP, legacy HTTP+SSE,
request guard and session cap, plus the same `reply()` helper. This file
says, for each one, what is deleted, what replaces it, and which
differences between the copies `mcp-serve` had to choose between.

Read against these trees:

| Server | Branch | Commit | Version in `Cargo.toml` |
| :--- | :--- | :--- | :--- |
| noyalib-mcp | `main` | `19990d2` | 0.0.58 |
| oxml-mcp | `feat/v0.0.16` | `df08404` | 0.0.16 |
| rlg (`crates/rlg-mcp`) | `feat/v0.0.18` | `7d8b16b` (plus uncommitted version bumps) | 0.0.18 |

None of the server repositories has been changed. `mcp-serve` is not
published, so each step below assumes it is reachable as a dependency,
either from crates.io once published or as a `path`/`git` dependency in
the meantime. That choice is the maintainer's.

## What every server gets

```toml
[dependencies]
mcp-serve = "0.0.1"
```

| Copy in the server | Replacement |
| :--- | :--- |
| `transport::run(name, version, args, factory)` | `mcp_serve::serve_main(name, version, args, factory)` |
| `transport::run` with server flags (noyalib) | `mcp_serve::serve_main_with` and a `ServerFlags` impl |
| `transport::parse`, `usage`, `serve`, `Options`, `Command`, `Transport` | the same names at the crate root; `Command::Serve` now carries `(Options, flags)` |
| `transport/bounded_lines.rs` | `mcp_serve::BoundedLines` (applied to stdio by `serve_main`) |
| `transport/guard.rs`, `sessions.rs`, `sse.rs` | internal to `mcp_serve` |
| `fn reply(outcome, is_error)` | `mcp_serve::reply(outcome, is_error)` |
| noyalib's `off_thread(limit, work)` and `DEFAULT_CALL_TIMEOUT` | `mcp_serve::run_blocking(limit, work)` and `mcp_serve::DEFAULT_CALL_TIMEOUT` |

The transport unit tests (`transport/tests.rs` and the `#[cfg(test)]`
modules of `guard.rs`, `sessions.rs`, `sse.rs`, `bounded_lines.rs`) are
ported to this crate and deleted with the module. Each server's own
`tests/http.rs` and stdio tests keep passing unchanged against its binary
(rlg-mcp's in-process `tests/http.rs` needs the two-line change below);
they become the consumer side of the contract.

`rmcp` must resolve to the same version in the server and in
`mcp-serve`, since the handler type crosses the boundary. A server can
name it through `mcp_serve::rmcp`, or keep its own `rmcp` requirement
compatible with `^3.6`.

---

## oxml-mcp

### Delete

- `src/transport.rs` (395 lines)
- `src/transport/bounded_lines.rs`, `guard.rs`, `sessions.rs`, `sse.rs`,
  `tests.rs` (896 lines)
- In `src/main.rs`: `mod transport;`
- In `src/lib.rs`, lines 465 to 495: the doc comment and `fn reply`.

### Replace

`src/main.rs`:

```rust
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    mcp_serve::serve_main(
        "oxml-mcp",
        env!("CARGO_PKG_VERSION"),
        std::env::args().skip(1),
        oxml_mcp::XmlServer::new,
    )
}
```

`src/lib.rs`, with the other imports:

```rust
use mcp_serve::reply;
```

The call sites (`reply(outcome, |r| ...)`) do not change.

### Cargo.toml

- Add `mcp-serve`.
- Remove from `[dependencies]`: `axum`, `futures`, `tokio-util`, `uuid`.
  Nothing outside `src/transport*` uses them.
- `tokio` in `[dependencies]` is used only by the transport; it can go
  too (the `[dev-dependencies]` entry stays for the tests). Confirm with
  `cargo machete` after the change.
- `rmcp` keeps `server` and `macros` for the tools; `transport-io` and
  `transport-streamable-http-server` now come through `mcp-serve`.

### Behaviour

None changes. oxml-mcp's copy was already the hardened one (bounded
stdio lines, guard, session cap); `mcp-serve`'s `origin_host` is its
form of the check.

---

## rlg-mcp (in the rlg workspace)

### Delete

- `crates/rlg-mcp/src/transport.rs` (422 lines)
- `crates/rlg-mcp/src/transport/bounded_lines.rs`, `guard.rs`,
  `sessions.rs`, `sse.rs`, `tests.rs` (937 lines)
- In `crates/rlg-mcp/src/lib.rs`, lines 31 to 35:

  ```rust
  // The same file in every Rust MCP server of the suite, so it keeps
  // its own formatting. Declared here rather than in `main.rs` so the
  // transports can be served, and tested, in-process.
  #[rustfmt::skip]
  pub mod transport;
  ```

- In `crates/rlg-mcp/src/lib.rs`, lines 85 to 114: the doc comment and
  `fn reply`. Its `use serde::Serialize;` and `use std::fmt;` may then be
  unused in that file; the compiler will say.

### Replace

`crates/rlg-mcp/src/main.rs`, the body of `main`:

```rust
fn main() -> ExitCode {
    mcp_serve::serve_main(
        "rlg-mcp",
        env!("CARGO_PKG_VERSION"),
        std::env::args().skip(1),
        rlg_mcp::LogServer::new,
    )
}
```

`crates/rlg-mcp/src/lib.rs`: `use mcp_serve::reply;`, and each of the four
call sites (lines 170, 193, 212, 235) gains the predicate the other two
servers already pass:

```rust
reply(filter_log_outcome(&args), |_| false)
```

`crates/rlg-mcp/tests/http.rs` serves in-process through the old module.
Two edits:

```rust
use rlg_mcp::transport;                 // delete
// ...
let mcp_serve::Command::Serve(options, _) =
    mcp_serve::parse(args).expect("valid arguments")
// ...
served = mcp_serve::serve(&options, rlg_mcp::LogServer::new) => {
```

and `mcp-serve` added to `[dev-dependencies]` (or used through the
normal dependency).

### Cargo.toml

- Add `mcp-serve`.
- Remove from `[dependencies]`: `axum`, `futures`, `tokio-util`, `uuid`,
  and, if `cargo machete` agrees, `tokio`.

### Behaviour

- **Public API.** `rlg_mcp::transport` was `pub`. Removing it is a
  breaking change for any code that called `rlg_mcp::transport::run` or
  `parse` directly. At 0.0.x that is allowed, but it belongs in the
  CHANGELOG. A `pub use mcp_serve as transport;` would not restore it:
  `run` is now `serve_main`, and `Command::Serve` has two fields.
- **`reply`.** rlg-mcp's copy took no `is_error` predicate and could not
  flag a successful value as an error. The shared one takes it; passing
  `|_| false` reproduces the old results exactly.

---

## noyalib-mcp

noyalib-mcp is the server with flags of its own (`--root`, `--profile`),
the only one that runs its tools on the blocking pool under a time limit,
and the only one whose stdio was not yet line-bounded.

### Delete

- `src/transport.rs` (506 lines). The root-handling functions
  (`resolve_root`, `refuse_broad_default`, `chosen_root`, `home_dir`,
  `parse_profile`) move to `src/flags.rs` below; everything else goes.
- `src/transport/guard.rs`, `sessions.rs`, `sse.rs` (663 lines)
- `src/transport/tests.rs` (167 lines). Its `root_is_a_path_option`,
  `profile_defaults_to_strict_and_takes_standard`,
  `a_missing_root_is_a_usage_error` and
  `a_broad_working_directory_is_not_a_default_root` move to
  `src/flags.rs` (adjusted to `parse_with::<Flags, _>` and
  `Command::Serve(_, flags)`); the rest are ported in this crate.
- In `src/main.rs`: `mod transport;`
- In `src/tools.rs`, lines 572 to 595: `DEFAULT_CALL_TIMEOUT` and
  `off_thread`.
- In `src/tools.rs`, lines 686 to 714: the doc comment and `fn reply`.

### Replace

`src/main.rs`, after the existing attributes:

```rust
mod flags;

use std::process::ExitCode;

#[cfg_attr(noyalib_coverage, coverage(off))]
fn main() -> ExitCode {
    mcp_serve::serve_main_with(
        "noyalib-mcp",
        env!("CARGO_PKG_VERSION"),
        std::env::args().skip(1),
        |flags: &flags::Flags| {
            noyalib_mcp::YamlServer::with_root(flags.root().to_path_buf())
                .with_profile(flags.profile)
        },
    )
}
```

`src/flags.rs` (new; the function bodies are the ones deleted from
`transport.rs`, unchanged except `chosen_root`, which now takes the
`--root` value instead of `&Options`):

```rust
//! The flags noyalib-mcp adds to the shared command line.

use std::path::{Path, PathBuf};

use mcp_serve::ServerFlags;
use noyalib_mcp::ParseProfile;

/// `--root` and `--profile`.
#[derive(Debug, Default)]
pub(crate) struct Flags {
    /// `--root` as given; `None` means the working directory.
    root: Option<PathBuf>,
    /// The root once checked by `validate`.
    resolved: Option<PathBuf>,
    /// The rules the parse tools apply.
    pub(crate) profile: ParseProfile,
}

impl Flags {
    /// The checked root. Set before any handler is made.
    pub(crate) fn root(&self) -> &Path {
        self.resolved.as_deref().unwrap_or(Path::new("."))
    }
}

impl ServerFlags for Flags {
    const NAMES: &'static [&'static str] = &["--root", "--profile"];
    const SYNOPSIS: &'static str =
        "[--root <dir>] [--profile <strict|standard>]";
    const HELP: &'static str = "\x20 --root <dir>        directory the file \
        tools may read and write (default: the working directory, unless \
        that is / or the home directory)\n\
        \x20 --profile <name>    rules every tool parses under: strict \
        (default) or standard\n";

    fn set(&mut self, flag: &str, value: String) -> Result<(), String> {
        match flag {
            "--root" => self.root = Some(PathBuf::from(value)),
            _ => self.profile = parse_profile(&value)?,
        }
        Ok(())
    }

    /// The root is checked once, here, so a typo is a usage error with
    /// a message instead of a server that refuses every file.
    fn validate(&mut self) -> Result<(), String> {
        self.resolved = Some(chosen_root(self.root.as_deref())?);
        Ok(())
    }
}

fn chosen_root(root: Option<&Path>) -> Result<PathBuf, String> {
    let resolved = resolve_root(root)?;
    if root.is_none() {
        refuse_broad_default(&resolved, home_dir().as_deref())?;
    }
    Ok(resolved)
}

// parse_profile, resolve_root, refuse_broad_default, home_dir: moved
// verbatim from src/transport.rs.
```

`src/tools.rs`:

```rust
pub use mcp_serve::DEFAULT_CALL_TIMEOUT;
use mcp_serve::{reply, run_blocking};
```

and each `off_thread(self.call_timeout, move || { ... })` becomes
`run_blocking(self.call_timeout, move || { ... })` (six call sites,
lines 757 to 920). The re-export keeps `noyalib_mcp::DEFAULT_CALL_TIMEOUT`
in the public API with the same value (30 s). `fn internal` stays: other
code in `tools.rs` uses it.

### Cargo.toml

- Add `mcp-serve`.
- Remove from `[dependencies]`: `axum`, `futures`, `tokio-util`.
  `uuid` stays (`src/fsio.rs` names temporary files with it). Outside
  the transport, `tokio` was used by `off_thread` and by
  `src/tools/tests.rs`; with `off_thread` gone it may only be needed as
  a dev-dependency. Confirm with `cargo machete`.
- Raise `rmcp` to `3.6` (it asks for `3.4`; the lockfile has 3.5.0) and
  `tokio` to `1.54` (lockfile 1.53.2): `mcp-serve` needs both floors.

### Behaviour

- **Stdio lines are now capped at 16 MiB.** noyalib-mcp read stdio
  through the SDK's own framing, which buffers an unended line until the
  pipe closes. oxml-mcp and rlg-mcp already capped it. A line past the
  cap ends the session. noyalib-mcp's own document limits apply to a
  tool argument after it has been read; the cap is what bounds the read
  itself.
- **`--help` lists `--root` and `--profile` after `--max-sessions`**, in
  both the usage line and the option list; before, they came between
  `--port` and `--max-sessions`. The text of each line is unchanged.
- **Exit statuses are unchanged**: a bad `--root` is still a usage error
  (status 2) printed with the usage text before anything is served.

---

## Differences between the copies, and the choice made

| Point | noyalib-mcp | oxml-mcp | rlg-mcp | mcp-serve | Why |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Stdio line cap | none | 16 MiB | 16 MiB | 16 MiB | The two hardened copies agree, and an unbounded line is an unbounded allocation. |
| `reply` signature | `(outcome, is_error)` | `(outcome, is_error)` | `(outcome)` | `(outcome, is_error)` | Two of three, and the predicate is needed by validation tools (a report with violations is an `isError` result that keeps its structure). `|_| false` gives the one-argument behaviour. |
| Per-call time limit | `off_thread` on the blocking pool, 30 s default | none | none | `run_blocking`, called by the tool, never by the transport | Opt-in: oxml-mcp and rlg-mcp tools run inline on the async worker, and wrapping them in a timer without moving them off the worker would not interrupt them. |
| Server flags | in `Options` | none | none | `ServerFlags` trait, kept by the server | The shared crate cannot know `ParseProfile`; the hook keeps noyalib's flags, parsing rules and validation in noyalib. |
| Flag position in `--help` | between `--port` and `--max-sessions` | n/a | n/a | after `--max-sessions` | The shared lines stay fixed and the server's are appended; text only. |
| `origin_host` | `scheme_str()?; host()` | `scheme_str().and(host())` | `scheme_str()?; host()` | `scheme_str().and(host())` | Same result for every input; this form does not discard a value, which `unused_results` would flag. |
| SSE session admission | inline in the handler | `admit` returning `Result` | `admit` returning `bool` | rlg-mcp's `admit` | Same status (503) and message in all three; rlg-mcp's is the most recent port. |
| `#[allow(dead_code)]` on the SSE stream's guard field | yes | yes | yes | field renamed `_guard`, no suppression | Same behaviour, no suppression. |
| `rmcp` floor | 3.4 | 3.6 | 3.4 | 3.6 | The highest of the three; verified by running the whole suite with `rmcp` pinned to 3.6.0 and to 3.9.0. |

Everything else in the transport (paths, defaults, error texts, status
codes, the `Host`/`Origin`/`Content-Type` rules, the session cap on both
`create_session` and `restore_session`, the trailing-slash route) was
identical in the three copies and is unchanged.

## agtmls-mcp

agtmls-mcp hand-rolls JSON-RPC over stdio with `BufRead::lines()`, which
has the unbounded-line problem this crate fixes, and has no HTTP
transport. Adopting `mcp-serve` means porting its handler to
`rmcp::ServerHandler` first, and taking on `rmcp`, `tokio` and `axum`
(about 120 crates in this crate's lockfile) where it now depends on
`serde` and `serde_json` alone. Whether HTTP transports are worth that
for agtmls-mcp is a product decision; if only the line cap is wanted,
the cap logic in `src/bounded_lines.rs` is small enough to copy as a
synchronous `Read` adapter.
