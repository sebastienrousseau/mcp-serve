// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! The process around a session: the stdio framing, the exit status,
//! and what is printed before anything is served.
//!
//! Ported from oxml-mcp's `tests/server.rs`, which drives its binary
//! over a real pipe. This crate has no binary, so the same
//! conversations run in-process over an in-memory pipe through
//! [`serve_io`], the function the stdio transport calls with the
//! process's own stdin and stdout.

use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use super::*;
use crate::bounded_lines::MAX_LINE_BYTES;
use crate::test_support::{Echo, NAME};

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#;
const INITIALIZED: &str =
    r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

/// Send `lines` to a stdio session, close its input, and collect the
/// replies. The session must end on its own, successfully.
async fn converse(lines: &[&str]) -> Vec<String> {
    let (mut client, server_in) = tokio::io::duplex(1 << 16);
    let (server_out, mut replies) = tokio::io::duplex(1 << 16);
    let session = tokio::spawn(serve_io(Echo, server_in, server_out));
    for line in lines {
        client.write_all(line.as_bytes()).await.expect("write");
        client.write_all(b"\n").await.expect("write");
    }
    // Closing the input is how a stdio client ends the session.
    drop(client);
    let mut out = String::new();
    let _ = replies.read_to_string(&mut out).await.expect("read");
    let ended = tokio::time::timeout(Duration::from_secs(10), session)
        .await
        .expect("the session ends at end of input")
        .expect("no panic");
    assert!(ended.is_ok(), "{ended:?}");
    out.lines().map(str::to_owned).collect()
}

/// The reply carrying `id`.
///
/// Requests are dispatched concurrently, so replies may come back in
/// any order; a client correlates them by id, and so must a test.
fn by_id(replies: &[String], id: u32) -> serde_json::Value {
    replies
        .iter()
        .map(|r| serde_json::from_str::<serde_json::Value>(r).expect("JSON"))
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no reply with id {id} in {replies:?}"))
}

#[tokio::test]
async fn one_request_draws_exactly_one_line() {
    let replies = converse(&[INITIALIZE]).await;
    assert_eq!(replies.len(), 1, "{replies:?}");
    let init = by_id(&replies, 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(init["result"]["serverInfo"]["name"], NAME);
}

#[tokio::test]
async fn an_older_client_is_answered_in_its_own_revision() {
    let replies = converse(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"old","version":"0"}}}"#,
    ])
    .await;
    assert_eq!(
        by_id(&replies, 1)["result"]["protocolVersion"],
        "2024-11-05"
    );
}

#[tokio::test]
async fn every_request_in_a_session_is_answered() {
    let replies = converse(&[
        INITIALIZE,
        INITIALIZED,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"Dune"}}}"#,
    ])
    .await;
    // Three requests, one notification: three replies.
    assert_eq!(replies.len(), 3, "{replies:?}");
    assert_eq!(by_id(&replies, 2)["result"]["tools"][0]["name"], "echo");
    assert_eq!(by_id(&replies, 3)["result"]["content"][0]["text"], "Dune");
}

#[tokio::test]
async fn tools_round_trip_annotations_output_schema_and_structured_content() {
    let replies = converse(&[
        INITIALIZE,
        INITIALIZED,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"Dune"}}}"#,
    ])
    .await;
    let tool = &by_id(&replies, 2)["result"]["tools"][0];
    assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
    assert_eq!(tool["annotations"]["destructiveHint"], false, "{tool}");
    assert_eq!(tool["annotations"]["idempotentHint"], true, "{tool}");
    assert_eq!(tool["annotations"]["openWorldHint"], false, "{tool}");
    assert_eq!(
        tool["outputSchema"]["properties"]["text"]["type"], "string",
        "{tool}"
    );
    let call = &by_id(&replies, 3)["result"];
    assert_eq!(call["structuredContent"]["text"], "Dune", "{call}");
}

#[tokio::test]
async fn a_stateless_client_needs_no_handshake() {
    let replies = converse(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}"#,
    ])
    .await;
    assert_eq!(replies.len(), 2, "{replies:?}");
    assert_eq!(by_id(&replies, 1)["result"]["tools"][0]["name"], "echo");
    let versions = by_id(&replies, 2)["result"]["supportedVersions"].clone();
    assert!(versions.to_string().contains("2026-07-28"), "{versions}");
    assert!(versions.to_string().contains("2025-11-25"), "{versions}");
}

#[tokio::test]
async fn blank_lines_are_skipped_rather_than_answered() {
    let replies = converse(&["", "   ", INITIALIZE, ""]).await;
    assert_eq!(replies.len(), 1, "{replies:?}");
}

#[tokio::test]
async fn a_notification_alone_draws_no_reply_and_ends_cleanly() {
    assert_eq!(converse(&[INITIALIZED]).await, Vec::<String>::new());
}

#[tokio::test]
async fn no_input_at_all_ends_successfully() {
    assert_eq!(converse(&[]).await, Vec::<String>::new());
}

#[tokio::test]
async fn malformed_input_does_not_stop_the_server() {
    let replies = converse(&[
        "this is not json",
        INITIALIZE,
        INITIALIZED,
        r#"{"jsonrpc":"2.0","id":7}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    ])
    .await;
    assert_eq!(replies.len(), 3, "{replies:?}");
    assert!(replies.iter().any(|r| r.contains("-32600")), "{replies:?}");
    assert_eq!(by_id(&replies, 2)["result"]["tools"][0]["name"], "echo");
}

#[tokio::test]
async fn an_unknown_method_and_tool_are_reported_distinctly() {
    let replies = converse(&[
        INITIALIZE,
        INITIALIZED,
        r#"{"jsonrpc":"2.0","id":2,"method":"no/such"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"no_such_tool","arguments":{}}}"#,
    ])
    .await;
    assert_eq!(replies.len(), 3, "{replies:?}");
    assert_eq!(by_id(&replies, 2)["error"]["code"], -32601);
    let tool = by_id(&replies, 3);
    assert_eq!(tool["result"]["isError"], true, "{tool}");
}

#[tokio::test]
async fn text_containing_newlines_stays_on_one_line() {
    let replies = converse(&[
        INITIALIZE,
        INITIALIZED,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"text":"a\nb\nc"}}}"#,
    ])
    .await;
    assert_eq!(replies.len(), 2, "{replies:?}");
    assert_eq!(
        by_id(&replies, 2)["result"]["content"][0]["text"],
        "a\nb\nc"
    );
}

#[tokio::test]
async fn an_oversized_unterminated_line_does_not_wait_for_end_of_input() {
    let (mut client, server_in) = tokio::io::duplex(1 << 16);
    let (server_out, _replies) = tokio::io::duplex(1 << 16);
    let session = tokio::spawn(serve_io(Echo, server_in, server_out));
    // The writer is kept open: the session must end on the cap, not on
    // end of input.
    let writer = tokio::spawn(async move {
        let line = vec![b' '; MAX_LINE_BYTES + 1];
        let wrote = client.write_all(&line).await;
        (client, wrote)
    });
    let ended = tokio::time::timeout(Duration::from_secs(10), session)
        .await
        .expect("an unbounded stdio line was buffered");
    assert!(ended.is_ok(), "{ended:?}");
    let (_client, wrote) = writer.await.expect("writer");
    if let Err(e) = wrote {
        assert_eq!(e.kind(), io::ErrorKind::BrokenPipe, "{e}");
    }
}

// --- Before anything is served --------------------------------------------

#[test]
fn version_and_help_are_printed_to_stdout() {
    assert_eq!(
        ready_to_serve::<NoFlags, _>("echo-mcp", "1.2.3", ["--version"]),
        Err(Early::Print("echo-mcp 1.2.3".to_owned()))
    );
    let Err(Early::Print(help)) =
        ready_to_serve::<NoFlags, _>("echo-mcp", "1.2.3", ["--help"])
    else {
        panic!("expected the usage text");
    };
    assert!(
        help.contains("--transport") && help.contains("/mcp"),
        "{help}"
    );
}

#[test]
fn a_bad_argument_is_a_usage_error_naming_it() {
    let Err(Early::Usage(text)) = ready_to_serve::<NoFlags, _>(
        "echo-mcp",
        "1",
        ["--transport", "telepathy"],
    ) else {
        panic!("expected a usage error");
    };
    assert!(text.starts_with("echo-mcp: "), "{text}");
    assert!(text.contains("telepathy"), "{text}");
    assert!(text.contains("Usage:"), "{text}");
}

/// Flags whose validation refuses one value and resolves the rest.
#[derive(Debug, Default, PartialEq, Eq)]
struct Root(String);

impl ServerFlags for Root {
    const NAMES: &'static [&'static str] = &["--root"];
    const SYNOPSIS: &'static str = "[--root <dir>]";
    const HELP: &'static str = "\x20 --root <dir>        the root\n";

    fn set(&mut self, _flag: &str, value: String) -> Result<(), String> {
        self.0 = value;
        Ok(())
    }

    fn validate(&mut self) -> Result<(), String> {
        if self.0 == "/missing" {
            return Err("--root /missing: not a directory".to_owned());
        }
        self.0.push_str("/resolved");
        Ok(())
    }
}

#[test]
fn validation_runs_before_serving_and_its_error_is_a_usage_error() {
    let ready = ready_to_serve::<Root, _>("yaml-mcp", "1", ["--root=/srv"]);
    assert_eq!(
        ready,
        Ok((Options::default(), Root("/srv/resolved".to_owned())))
    );
    let Err(Early::Usage(text)) =
        ready_to_serve::<Root, _>("yaml-mcp", "1", ["--root", "/missing"])
    else {
        panic!("expected a usage error");
    };
    assert!(text.contains("not a directory"), "{text}");
    assert!(text.contains("[--root <dir>]"), "{text}");
    // Help is not validated.
    assert!(matches!(
        ready_to_serve::<Root, _>("yaml-mcp", "1", ["--root=/missing", "-h"]),
        Err(Early::Print(_))
    ));
}

#[test]
fn the_exit_status_says_how_the_process_ended() {
    let status = |args: &[&str]| {
        exit_status("echo-mcp", "1", args.to_vec(), |_: &NoFlags| Echo)
    };
    assert_eq!(status(&["--version"]), 0);
    assert_eq!(status(&["--bogus"]), 2);
    // A port already taken: a transport failure, reported, status 1.
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = holder.local_addr().expect("addr").port().to_string();
    assert_eq!(
        status(&["--transport", "streamable-http", "--port", &port]),
        1
    );
    assert_eq!(status(&["--transport", "sse", "--port", &port]), 1);
}

#[tokio::test]
async fn a_port_in_use_is_reported_not_swallowed() {
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = holder.local_addr().expect("addr").port();
    let options = Options {
        transport: Transport::StreamableHttp,
        port,
        ..Options::default()
    };
    let err = serve(&options, || Echo).await.expect_err("in use");
    let text = err.to_string();
    assert!(text.contains("cannot listen on"), "{text}");
    assert!(text.contains(&port.to_string()), "{text}");
}

#[test]
fn serve_main_returns_an_exit_code() {
    // `ExitCode` has no equality; its debug form names the status.
    let code = serve_main("echo-mcp", "1", ["--version"], || Echo);
    assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::SUCCESS));
}
