// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! The transport half of a Rust MCP server.
//!
//! A server built on [`rmcp`] writes its tools as a [`ServerHandler`];
//! this crate serves that handler. Every server that uses it is started
//! the same way:
//!
//! ```text
//! <server>                                   # stdio, for a client that spawns it
//! <server> --transport streamable-http       # HTTP on 127.0.0.1:8000, path /mcp
//! <server> --transport sse --port 8001       # the older HTTP+SSE transport
//! ```
//!
//! Over streamable HTTP the SDK speaks both current protocol revisions
//! on one endpoint: `2026-07-28` (stateless, `server/discover`,
//! per-request `_meta`) and `2025-11-25` (`initialize` handshake,
//! `Mcp-Session-Id`). Responses stream as server-sent events; a `GET`
//! on the same path opens the server-to-client event stream.
//! `--transport sse` serves the `2024-11-05` HTTP+SSE transport for
//! clients that still expect it: `GET /sse` opens the event stream,
//! whose first event names the `/messages/?sessionId=...` endpoint the
//! client posts to.
//!
//! The listener binds the loopback interface unless told otherwise.
//! There is no authentication here: put the server behind a gateway
//! you trust before binding a routable address. Both HTTP transports
//! refuse a `Host` the server does not answer to and a non-local
//! `Origin`, which is what a browser page reaching the server through
//! DNS rebinding sends; the legacy transport also refuses a posted
//! message that is not `application/json`. Both hold at most
//! `--max-sessions` sessions at once. Over stdio, one line may not
//! pass [`MAX_LINE_BYTES`] (16 MiB).
//!
//! Nothing here knows what the tools are. Two helpers are for the
//! tools themselves: [`reply`] builds the result every tool returns,
//! and [`run_blocking`] is the opt-in per-call time limit for tools
//! that block.
//!
//! # Examples
//!
//! A whole server binary, with no flags of its own:
//!
//! ```no_run
//! use rmcp::ServerHandler;
//!
//! #[derive(Clone)]
//! struct Server;
//! impl ServerHandler for Server {}
//!
//! fn main() -> std::process::ExitCode {
//!     mcp_serve::serve_main(
//!         "my-mcp",
//!         env!("CARGO_PKG_VERSION"),
//!         std::env::args().skip(1),
//!         || Server,
//!     )
//! }
//! ```
//!
//! A server with flags of its own implements [`ServerFlags`] and calls
//! [`serve_main_with`].
//!
//! [`ServerHandler`]: rmcp::ServerHandler

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bounded_lines;
mod cli;
mod guard;
mod reply;
mod serve;
mod sessions;
mod sse;
mod timeout;

#[cfg(test)]
#[path = "../tests/support/echo.rs"]
mod test_support;

pub use bounded_lines::{BoundedLines, MAX_LINE_BYTES};
pub use cli::{
    Command, DEFAULT_HOST, DEFAULT_MAX_SESSIONS, DEFAULT_PORT, MESSAGE_PATH,
    NoFlags, Options, SSE_PATH, STREAMABLE_HTTP_PATH, ServerFlags, Transport,
    parse, parse_with, usage, usage_with,
};
pub use reply::reply;
pub use serve::{serve, serve_main, serve_main_with};
pub use timeout::{DEFAULT_CALL_TIMEOUT, run_blocking};

/// The SDK this crate serves, re-exported so a server can name the
/// exact version the transports were built against.
pub use rmcp;
