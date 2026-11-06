// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! The two HTTP transports, end to end.
//!
//! Ported from rlg-mcp's `tests/http.rs`, which carries the cases of
//! oxml-mcp's and noyalib-mcp's, against a one-tool server instead of
//! a real one. Each server runs in-process on a free port and is
//! talked to over HTTP/1.1 on a plain socket. The client is
//! deliberately small and literal, one request and one response,
//! chunked bodies decoded by hand, because what is being checked is
//! the wire format a client will see, and a client library would hide
//! it.

#[path = "../support/echo.rs"]
mod echo;

mod client;
mod sse;
mod streamable;
