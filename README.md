<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

<h1 align="center">mcp-serve</h1>

<p align="center">
  The transport half of a Rust MCP server: stdio, streamable HTTP and legacy HTTP+SSE behind one command line, hardened once.
</p>

<p align="center">
  <a href="https://github.com/sebastienrousseau/mcp-serve/actions"><img src="https://img.shields.io/github/actions/workflow/status/sebastienrousseau/mcp-serve/ci.yml?style=for-the-badge&logo=github" alt="Build" height="28" /></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg?style=for-the-badge" alt="License: MIT OR Apache-2.0" height="28" /></a>
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/rustc-1.89%2B-93450a.svg?style=for-the-badge&logo=rust" alt="Minimum Rust 1.89" height="28" /></a>
</p>

No demo recording yet: the crate is a library with no binary of its own.
`cargo run --example greeter -- --help` shows what a server built on it
prints.

---

## Contents

**Getting started**

- [Install](#install) — a git dependency until the crate is published
- [Requirements](#requirements) — toolchain floor, platforms
- [Quick Start](#quick-start) — a whole server `main` in one call

**The mcp-serve ecosystem**

- [The mcp-serve ecosystem](#the-mcp-serve-ecosystem) — noyalib-mcp, oxml-mcp, rlg-mcp

**Library reference**

- [Capabilities at a glance](#capabilities-at-a-glance) — the current surface by theme
- [Ecosystem comparison](#ecosystem-comparison) — short matrix
- [Benchmarks](#benchmarks) — none measured yet
- [Features](#features) — module-level capability list
- [Configuration](#configuration) — core options
- [Examples](#examples) — runnable example index

**Operational**

- [When not to use mcp-serve](#when-not-to-use-mcp-serve) — limitations
- [Development](#development) — make targets, CI
- [Security](#security) — guarantees and compliance
- [Documentation](#documentation) — all reference docs
- [Stability guarantees](#stability-guarantees) — SemVer axis, output stability, minimum toolchain discipline
- [License](#license)

---

## Install

### As a Rust library

```toml
[dependencies]
mcp-serve = { git = "https://github.com/sebastienrousseau/mcp-serve", rev = "<commit>" }
```

The crate is not on crates.io yet, so it is a git dependency pinned to a
full commit hash on `main`. A git dependency cannot be published, so a
server that depends on it cannot publish its own release until this
crate is on crates.io; then the line becomes `mcp-serve = "0.0.2"`.
[`MIGRATION.md`](MIGRATION.md) has the `cargo deny` entry it needs
meanwhile.

---

## Requirements

- Rust 1.89 or later (edition 2024), checked by `make msrv`.
- A Tokio runtime is created by `serve_main`; a server that calls
  `serve` directly brings its own.
- `rmcp` 3.6 or later, the same version the server's handler is written
  against. The crate re-exports it as `mcp_serve::rmcp`.
- Developed and tested on macOS (Apple Silicon). Nothing in the crate is
  platform-specific, but no other platform has been run.

---

## Quick Start

```rust
use rmcp::ServerHandler;

#[derive(Clone)]
struct Server;
impl ServerHandler for Server {}

fn main() -> std::process::ExitCode {
    mcp_serve::serve_main(
        "my-mcp",
        env!("CARGO_PKG_VERSION"),
        std::env::args().skip(1),
        || Server,
    )
}
```

That binary now answers to:

```text
my-mcp                                   # stdio, for a client that spawns it
my-mcp --transport streamable-http       # HTTP on 127.0.0.1:8000, path /mcp
my-mcp --transport sse --port 8001       # the 2024-11-05 HTTP+SSE transport
my-mcp --help | --version
```

A usage error exits with 2, a transport failure with 1, and a stdio
session the client ends with 0.

---

## The mcp-serve ecosystem

The crate was extracted from three MCP servers that each carried a copy of
the same transport module, about 1,300 lines apiece, which had drifted
(one copy read stdio unbounded). [`MIGRATION.md`](MIGRATION.md) lists, per
server, the code that goes and the call that replaces it.

| Component | Purpose | Use case |
| :--- | :--- | :--- |
| noyalib-mcp | Lossless YAML editing tools | Adds `--root` and `--profile` through `ServerFlags`; uses `run_blocking` |
| oxml-mcp | XML parsing, XPath and XSD tools | `serve_main` and `reply` |
| rlg-mcp | Log-stream tools for the rlg logger | `serve_main` and `reply` |
| agtmls-mcp | AgtMLS tools over hand-rolled stdio JSON-RPC | Possible consumer once ported to `rmcp` |

---

## Capabilities at a glance

| Area | Capability | Status |
| :--- | :--- | :--- |
| Entry point | `serve_main`, `serve_main_with`: command line to exit status | Tested |
| Command line | `--transport`, `--host`, `--port`, `--max-sessions`, `--help`, `--version`, `--flag=value` | Tested |
| Server flags | `ServerFlags`: value-taking flags and switches (`--dry-run`), listed in `--help`, validated before serving | Tested |
| Stdio | SDK framing over `BoundedLines`, 16 MiB per line | Tested |
| Streamable HTTP | MCP 2025-11-25 sessions and 2026-07-28 stateless requests on `/mcp` | Tested |
| Legacy HTTP+SSE | MCP 2024-11-05 on `/sse` and `/messages/` | Tested |
| Request guard | Foreign `Host` and `Origin` refused (403); legacy posts must be `application/json` (415) | Tested |
| Session cap | `--max-sessions` on both HTTP transports (503 on SSE) | Tested |
| Tool helpers | `reply` (text plus structured content), `run_blocking` (opt-in time limit) | Tested |

---

## Ecosystem comparison

The comparison that matters here is with the copies this crate replaces.

| Copy | Stdio line cap | Host/Origin guard and session cap | `reply` takes `is_error` |
| :--- | :---: | :---: | :---: |
| **mcp-serve** | yes | yes | yes |
| noyalib-mcp 0.0.58 | no | yes | yes |
| oxml-mcp 0.0.16 | yes | yes | yes |
| rlg-mcp 0.0.18 | yes | yes | no |

[`MIGRATION.md`](MIGRATION.md) has every difference and the choice made.
There is no `docs/COMPARISON.md`: no comparison with other crates has
been made.

---

## Benchmarks

None measured. The transports add a per-byte line count on stdio and a
header check per HTTP request on top of the SDK; no number has been
recorded for either, so none is claimed. There is no `docs/BENCHMARKS.md`.

| Scenario | Result | Environment |
| :--- | ---: | :--- |
| Not measured | n/a | n/a |

---

## Features

- `serve_main(name, version, args, factory)`: the whole of `main`.
- `serve_main_with::<Flags, ..>(..)`: the same, with the server's own
  flags; the factory receives them.
- `serve(&options, factory)`: serve inside a runtime the caller owns.
- `parse`, `parse_with`, `usage`, `usage_with`: the command line alone.
- `BoundedLines`, `MAX_LINE_BYTES`: the stdio line cap, usable over any
  `AsyncRead`.
- `reply(outcome, is_error)`: a `CallToolResult` with text and
  structured content, or an `isError` text result.
- `run_blocking(limit, work)`, `DEFAULT_CALL_TIMEOUT`: run blocking tool
  work on the blocking pool and answer with an error past the limit.

There are no Cargo features.

---

## Configuration

| Flag | Default | Effect |
| :--- | :--- | :--- |
| `--transport <stdio\|streamable-http\|sse>` | `stdio` | Which transport to serve |
| `--host <address>` | `127.0.0.1` | Interface the HTTP transports bind; also accepted in `Host` and `Origin` |
| `--port <number>` | `8000` | Port the HTTP transports bind; `0` asks the system for one |
| `--max-sessions <n>` | `64` | Sessions an HTTP transport holds at once |

Binding `0.0.0.0` or `::` turns the `Host` check off (there is no single
name to check against); the `Origin` check stays on. The listening
address is printed to stderr, so stdout stays free for stdio.

---

## Examples

- [`examples/greeter.rs`](examples/greeter.rs): a complete server with
  one tool and one flag of its own (`--greeting`), using `reply` and
  `run_blocking`. Run it with
  `cargo run --example greeter -- --transport streamable-http`.

---

## When not to use mcp-serve

- **Your server needs authentication.** Neither HTTP transport
  authenticates. Keep them on loopback or behind a gateway you trust.
- **You want an MCP client.** This is server-side only.
- **Your server is not built on `rmcp`.** The crate serves an
  `rmcp::ServerHandler`; a hand-rolled JSON-RPC loop has to be ported
  first.
- **You need to change a shared flag, or a richer command line.**
  The shared flags always keep their shared meaning. `ServerFlags`
  adds value-taking flags and switches only: no short forms, no
  `--no-` negation, no repeated values collected into a list.
- **Dependency weight matters more than HTTP.** The HTTP transports bring
  `axum` and `rmcp`'s HTTP server: about 120 crates in the lockfile.

---

## Development

```bash
make check        # the whole gate: fmt, clippy, complexity, test, doc, msrv, deny, audit
make test         # unit, integration and doc tests
make complexity   # per-function and per-file ceilings
make msrv         # cargo +1.89.0 check
```

Every target fails on a warning. `make complexity` holds every function to
cyclomatic 10, cognitive 15, Halstead difficulty 30 and 60 lines, and every
file to 500 lines, measured with `rust-code-analysis-cli`; there is no
baseline because nothing is over. On macOS 27, Rust 1.89's build scripts
fail to link against the 27.x SDK; `make msrv SDKROOT=<a MacOSX26 SDK>`
works around it locally.

CI (`.github/workflows/ci.yml`) runs on every pull request and on
`main`: format, clippy, tests on Linux, macOS and Windows, and
`cargo audit` through the shared `rust-ci.yml` workflow, plus
`cargo deny`, the complexity ceilings, the MSRV build, docs with
warnings denied, and a sign-off check on every commit.

---

## Security

- `#![forbid(unsafe_code)]`.
- HTTP requests whose `Host` is not one the server answers to, or whose
  `Origin` is not local, are refused before any handler runs. This is the
  defence against a browser page reaching a loopback server through DNS
  rebinding.
- The legacy transport refuses posts that are not `application/json`, so
  a page cannot send one cross-origin without a preflight.
- Live sessions are capped; one stdio line is capped at 16 MiB.
- `cargo deny` (licences, advisories, sources) and `cargo audit` run in
  `make check`.

Report a vulnerability privately, as [`SECURITY.md`](SECURITY.md)
describes, never in a public issue.

---

## Documentation

- API documentation: `cargo doc --open` (not yet on docs.rs).
- [`MIGRATION.md`](MIGRATION.md): moving noyalib-mcp, oxml-mcp and
  rlg-mcp onto the crate.
- [`CHANGELOG.md`](CHANGELOG.md): what each version changed.
- [`SECURITY.md`](SECURITY.md): reporting a vulnerability, threat model.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) and
  [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

---

## Stability guarantees

- Versions follow `0.0.x`, one step at a time; at 0.0.x any release may
  change the API, and the CHANGELOG says how.
- The command line, the paths (`/mcp`, `/sse`, `/messages/`), the
  defaults, the status codes and the error texts are pinned by tests,
  since clients and the servers' own tests depend on them.
- Minimum Rust 1.89, checked by `make msrv`; raising it is a CHANGELOG
  entry.

---

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE),
at your option, the same as the servers the code was extracted from.
