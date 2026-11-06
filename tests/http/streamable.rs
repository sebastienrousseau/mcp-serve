// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! Streamable HTTP: the 2025-11-25 session and the 2026-07-28
//! stateless revision on one endpoint.

use serde_json::json;

use crate::client::{
    ACCEPT_BOTH, EVIL_HOST, EVIL_ORIGIN, Http, JSON, STATELESS, Server,
    initialize, stateless,
};
use crate::echo::NAME;

const VERSION: (&str, &str) = ("MCP-Protocol-Version", "2025-11-25");

/// Open a 2025-11-25 session; its id.
fn handshake(server: &Server) -> String {
    let mut r =
        server.post_mcp(&[ACCEPT_BOTH, JSON], &initialize("2025-11-25"));
    assert_eq!(r.status, 200);
    assert!(r.is_event_stream(), "{:?}", r.headers);
    let session = r.header("mcp-session-id").expect("session id").to_owned();
    let init = r.next_message();
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(init["result"]["serverInfo"]["name"], NAME);
    session
}

#[test]
fn streamable_http_holds_a_session_with_a_handshake() {
    let server = Server::start("streamable-http");
    let session = handshake(&server);
    let id = ("Mcp-Session-Id", session.as_str());
    let r = server.post_mcp(
        &[ACCEPT_BOTH, JSON, id, VERSION],
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    assert_eq!(r.status, 202);

    let mut r = server.post_mcp(
        &[ACCEPT_BOTH, JSON, id, VERSION],
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    assert_eq!(r.status, 200);
    let list = r.next_message();
    let tools = list["result"]["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 1, "{list}");
    assert_eq!(tools[0]["name"], "echo", "{list}");
    drop(r);

    // GET opens the server-to-client stream for the session.
    let accept = ("Accept", "text/event-stream");
    let r = server.send("GET", "/mcp", &[accept, id, VERSION], "");
    assert_eq!(r.status, 200);
    assert!(r.is_event_stream(), "{:?}", r.headers);
}

#[test]
fn streamable_http_ends_a_session_and_refuses_unknown_ones() {
    let server = Server::start("streamable-http");
    let session = handshake(&server);
    let id = ("Mcp-Session-Id", session.as_str());
    // A session id the server never issued is not found.
    let r = server.post_mcp(
        &[ACCEPT_BOTH, JSON, ("Mcp-Session-Id", "nope"), VERSION],
        r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
    );
    assert_eq!(r.status, 404);
    // DELETE ends the session.
    let r = server.send("DELETE", "/mcp", &[id, VERSION], "");
    assert!(r.status < 300, "{}", r.status);
}

/// A stateless request to `server`, routed by `method` (and `tool`).
fn stateless_post(
    server: &Server,
    method: &str,
    tool: &str,
    body: &str,
) -> Http {
    let mut headers =
        vec![ACCEPT_BOTH, JSON, STATELESS, ("Mcp-Method", method)];
    if !tool.is_empty() {
        headers.push(("Mcp-Name", tool));
    }
    server.post_mcp(&headers, body)
}

#[test]
fn streamable_http_serves_stateless_requests_without_a_session() {
    let server = Server::start("streamable-http");
    let discover = stateless(1, "server/discover", json!({}));
    let mut r = stateless_post(&server, "server/discover", "", &discover);
    assert_eq!(r.status, 200);
    assert!(r.header("mcp-session-id").is_none(), "no session here");
    let discover = r.next_message();
    let versions = discover["result"]["supportedVersions"]
        .as_array()
        .expect("supportedVersions");
    assert!(versions.contains(&json!("2026-07-28")), "{discover}");
    assert!(versions.contains(&json!("2025-11-25")), "{discover}");
    let meta = &discover["result"]["_meta"];
    assert_eq!(meta["io.modelcontextprotocol/serverInfo"]["name"], NAME);
    drop(r);

    let list = stateless(2, "tools/list", json!({}));
    let mut r = stateless_post(&server, "tools/list", "", &list);
    assert_eq!(r.status, 200);
    let list = r.next_message();
    assert_eq!(list["result"]["tools"].as_array().map(Vec::len), Some(1));
}

#[test]
fn streamable_http_answers_stateless_tool_calls() {
    let server = Server::start("streamable-http");
    let args = json!({"name": "echo", "arguments": {"text": "Dune"}});
    let call = stateless(3, "tools/call", args);
    let mut r = stateless_post(&server, "tools/call", "echo", &call);
    assert_eq!(r.status, 200);
    let call = r.next_message();
    assert_eq!(call["result"]["isError"], false, "{call}");
    assert_eq!(call["result"]["content"][0]["text"], "Dune", "{call}");
    drop(r);

    // A tool failure is a result the model can read.
    let args = json!({"name": "echo", "arguments": {}});
    let call = stateless(4, "tools/call", args);
    let mut r = stateless_post(&server, "tools/call", "echo", &call);
    let call = r.next_message();
    assert_eq!(call["result"]["isError"], true, "{call}");
}

#[test]
fn streamable_http_refuses_what_the_stateless_revision_forbids() {
    let server = Server::start("streamable-http");
    // A request naming its revision without the rest of the required
    // metadata is refused before any handler runs.
    let r = stateless_post(
        &server,
        "tools/list",
        "",
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
    );
    assert_eq!(r.status, 400);
    assert!(r.body().contains("clientCapabilities"));

    // Without a session, GET has nothing to stream: 405.
    let accept = ("Accept", "text/event-stream");
    let r = server.send("GET", "/mcp", &[accept, STATELESS], "");
    assert_eq!(r.status, 405);

    // A body that claims to be JSON and is not: 415 from the SDK.
    let r = server.post_mcp(&[ACCEPT_BOTH, JSON, STATELESS], "{not json");
    assert_eq!(r.status, 415);
}

#[test]
fn streamable_http_refuses_a_mismatched_routing_header() {
    // SEP-2243: a header that disagrees with the body is a lie one of
    // the two is telling, and is refused.
    let server = Server::start("streamable-http");
    let body = stateless(2, "server/discover", json!({}));
    let r = stateless_post(&server, "tools/list", "", &body);
    assert_eq!(r.status, 400, "a mismatched Mcp-Method must be refused");
    assert!(r.body().contains("-32020"));
}

#[test]
fn streamable_http_refuses_a_foreign_host_or_origin() {
    let server = Server::start("streamable-http");
    for bad in [EVIL_ORIGIN, EVIL_HOST] {
        let r = server
            .post_mcp(&[ACCEPT_BOTH, JSON, bad], &initialize("2025-11-25"));
        assert_eq!(r.status, 403, "{bad:?}");
    }
    let local = ("Origin", "http://127.0.0.1:9");
    let r =
        server.post_mcp(&[ACCEPT_BOTH, JSON, local], &initialize("2025-11-25"));
    assert_eq!(r.status, 200);
}

#[test]
fn streamable_http_caps_the_number_of_live_sessions() {
    let server =
        Server::start_with("streamable-http", &["--max-sessions", "2"]);
    let open =
        || server.post_mcp(&[ACCEPT_BOTH, JSON], &initialize("2025-11-25"));
    assert_eq!(open().status, 200);
    assert_eq!(open().status, 200);
    let third = open();
    assert_ne!(third.status, 200, "a third session was created");
    assert!(third.body().contains("session limit"));
    // A request with no session (the stateless revision) still works.
    let discover = stateless(7, "server/discover", json!({}));
    let r = stateless_post(&server, "server/discover", "", &discover);
    assert_eq!(r.status, 200);
}
