<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Security Policy

## Supported versions

mcp-serve is pre-1.0. Only the most recent `0.0.x` release receives
security fixes.

## Reporting a vulnerability

Please report privately, not as a public issue:

- GitHub's [private vulnerability reporting](https://github.com/sebastienrousseau/mcp-serve/security/advisories/new)
- or email <sebastian.rousseau@gmail.com>

You should get an acknowledgement within 72 hours.

## Threat model

mcp-serve is the transport half of an MCP server: it reads the command
line, serves stdio, streamable HTTP or the 2024-11-05 HTTP+SSE
transport, and hands each session to the server's own
`rmcp::ServerHandler`. What the tools do with their input is the
server's responsibility, not this crate's.

- **Stdio** inherits the trust boundary of the process that launched
  the server. Each line is capped at 16 MiB before the SDK sees it.
- **HTTP transports** have no authentication and bind to `127.0.0.1` by
  default. A request whose `Host` is not one the server answers to, or
  whose `Origin` is not local, is refused with 403 before any handler
  runs: the defence against a browser page reaching a loopback server
  through DNS rebinding. Binding `0.0.0.0` or `::` turns the `Host`
  check off; the `Origin` check stays on.
- **The legacy transport** accepts `application/json` posts only (415
  otherwise), so a page cannot send one cross-origin without a
  preflight.
- **Sessions** are capped by `--max-sessions` on both HTTP transports.
  The cap does not bound the number of requests within a session or
  the total memory of the process.

A deployment that exposes either HTTP transport beyond loopback must
supply its own authentication and TLS, for example a reverse proxy.

**Memory safety.** The crate is `#![forbid(unsafe_code)]`. Its
dependencies (`rmcp`, `tokio`, `axum`) carry their own obligations.
This does not rule out resource exhaustion, logic defects or protocol
errors.

## What is not a vulnerability

- An HTTP transport reachable without authentication when it was bound
  to a non-loopback address. That is documented behaviour; put it
  behind a gateway you trust.
- A tool of a server built on this crate misbehaving. Report it to that
  server's repository.
