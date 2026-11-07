<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Changelog

All notable changes to this crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow the `0.0.x` increment policy of the repositories it serves.

## [Unreleased]

## [0.0.1]

### Added

- `serve_main` and `serve_main_with`: the whole of a server's `main`,
  from the command line to the exit status (0, 1 for a transport
  failure, 2 for a usage error).
- `ServerFlags`: a server's own flags, read alongside the shared ones,
  listed in `--help`, and validated before serving. `NAMES` take a value
  (`--flag value`, `--flag=value`); `SWITCHES` take none (`--flag`
  alone, `--flag=value` refused) and go to `set_switch`, which, like
  `SWITCHES`, has a default, so a server without switches adds
  nothing. A shared flag named in either list keeps its shared meaning.
- The shared command line (`--transport`, `--host`, `--port`,
  `--max-sessions`, `--help`, `--version`) as `parse`, `parse_with`,
  `usage` and `usage_with`.
- The stdio transport, each line capped at 16 MiB (`BoundedLines`).
- The streamable HTTP transport (MCP 2025-11-25 and 2026-07-28) and the
  2024-11-05 HTTP+SSE transport, both refusing a foreign `Host` or
  `Origin` and holding at most `--max-sessions` sessions; the legacy
  transport also takes `application/json` posts only.
- `reply`: the tool result carrying text and structured content.
- `run_blocking` and `DEFAULT_CALL_TIMEOUT`: the opt-in time limit for a
  tool that blocks.

Extracted from the transport modules of noyalib-mcp 0.0.58 (`main`),
oxml-mcp 0.0.16 (`feat/v0.0.16`) and rlg-mcp 0.0.18 (`feat/v0.0.18`);
`MIGRATION.md` says what each server drops and which behaviour
differences were resolved, and how.
