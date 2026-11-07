<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Contributing to mcp-serve

Thank you for considering a contribution. mcp-serve is the shared
transport of noyalib-mcp, oxml-mcp and rlg-mcp, so a change here reaches
all three: keep it small and say which server it is for.

## Ground rules

- **No `unsafe`.** The crate is `#![forbid(unsafe_code)]`.
- **Every change is tested.** A bug fix comes with a test that fails
  before it and passes after. "It compiles" is necessary, not
  sufficient.
- **Complexity ceilings.** Every function stays within cyclomatic 10,
  cognitive 15, Halstead difficulty 30 and 60 lines, and every file
  within 500 lines; `make complexity` checks them.
- **The command line is a contract.** Flags, defaults, paths, status
  codes and error texts are pinned by tests because clients and the
  servers' own tests depend on them. Changing one is a CHANGELOG entry.

## Getting set up

```bash
git clone https://github.com/sebastienrousseau/mcp-serve
cd mcp-serve
make test
```

## Before opening a pull request

```bash
make check
```

That is the whole gate: fmt, clippy, complexity, tests, docs, the MSRV
build, `cargo deny` and `cargo audit`. CI runs the same checks. It needs
`rust-code-analysis-cli`, `cargo-deny`, `cargo-audit` and the 1.89.0
toolchain installed.

## Signing off your work

Every commit must carry a `Signed-off-by` line:

```bash
git commit -s
```

The line certifies the [Developer Certificate of Origin](DCO): that you
wrote the change or have the right to submit it under this project's
licence. CI checks every commit a pull request adds; bot commits are
exempt.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/). The body
explains what was wrong and why the change is right, not only what
changed.

## Reporting a security issue

Please do not open a public issue. See [SECURITY.md](SECURITY.md).
