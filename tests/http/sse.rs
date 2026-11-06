// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! The 2024-11-05 HTTP+SSE transport.

use serde_json::json;

use crate::client::{
    EVIL_HOST, EVIL_ORIGIN, JSON, PING, Server, eventually, initialize,
};
use crate::echo::NAME;

#[test]
fn sse_opens_a_stream_and_answers_posts_on_it() {
    let server = Server::start("sse");
    let (mut stream, endpoint) = server.open_sse();
    assert!(stream.is_event_stream(), "{:?}", stream.headers);
    assert!(endpoint.starts_with("/messages/?sessionId="), "{endpoint}");
    let post = |body: &str| server.send("POST", &endpoint, &[JSON], body);

    assert_eq!(post(&initialize("2024-11-05")).status, 202);
    let init = stream.next_message();
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(init["result"]["serverInfo"]["name"], NAME);

    let initialized =
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    assert_eq!(post(initialized).status, 202);
    let list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    assert_eq!(post(list).status, 202);
    let list = stream.next_message();
    assert_eq!(list["id"], 2);
    assert_eq!(list["result"]["tools"].as_array().map(Vec::len), Some(1));

    let call = json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"echo","arguments":{"text":"boom"}}});
    assert_eq!(post(&call.to_string()).status, 202);
    let call = stream.next_message();
    assert_eq!(call["result"]["content"][0]["text"], "boom", "{call}");
}

#[test]
fn sse_takes_the_message_path_with_or_without_its_slash() {
    let server = Server::start("sse");
    let (mut stream, endpoint) = server.open_sse();
    let bare = endpoint.replacen("/messages/?", "/messages?", 1);
    let ping = r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#;
    let r = server.send("POST", &bare, &[JSON], ping);
    assert_eq!(r.status, 202);
    assert_eq!(stream.next_message()["id"], 4);
}

#[test]
fn sse_refuses_what_it_cannot_route() {
    let server = Server::start("sse");
    let r = server.send("POST", "/messages/?sessionId=unknown", &[JSON], PING);
    assert_eq!(r.status, 404);

    let (stream, endpoint) = server.open_sse();
    let r = server.send("POST", &endpoint, &[JSON], "{not json");
    assert_eq!(r.status, 400);
    assert!(r.body().contains("invalid JSON-RPC"));

    // Hanging up ends the session: the endpoint stops existing.
    drop(stream);
    let gone = eventually(404, || {
        server.send("POST", &endpoint, &[JSON], PING).status
    });
    assert_eq!(gone, 404, "the session outlived its stream");
}

#[test]
fn sse_refuses_a_foreign_host_or_origin() {
    // A page that rebound its own name to 127.0.0.1 carries that name
    // in `Host` and its own origin in `Origin`. Neither route may
    // serve it.
    let server = Server::start("sse");
    for bad in [EVIL_HOST, EVIL_ORIGIN] {
        let r = server.send("GET", "/sse", &[bad], "");
        assert_eq!(r.status, 403, "GET /sse with {bad:?}");
    }
    let (mut stream, endpoint) = server.open_sse();
    for bad in [EVIL_HOST, EVIL_ORIGIN] {
        let r = server.send("POST", &endpoint, &[bad, JSON], PING);
        assert_eq!(r.status, 403, "POST with {bad:?}");
    }
    // A loopback origin is a local client.
    let own = ("Origin", "http://localhost:1234");
    let r = server.send("POST", &endpoint, &[own, JSON], PING);
    assert_eq!(r.status, 202);
    assert_eq!(stream.next_message()["id"], 1);
}

#[test]
fn sse_takes_json_posts_only() {
    // `text/plain` is what a browser may send cross-origin without a
    // preflight, so it is refused rather than parsed.
    let server = Server::start("sse");
    let (mut stream, endpoint) = server.open_sse();
    for headers in [&[("Content-Type", "text/plain")][..], &[][..]] {
        let r = server.send("POST", &endpoint, headers, PING);
        assert_eq!(r.status, 415, "{headers:?}");
    }
    let charset = ("Content-Type", "application/json; charset=utf-8");
    let r = server.send("POST", &endpoint, &[charset], PING);
    assert_eq!(r.status, 202);
    assert_eq!(stream.next_message()["id"], 1);
}

#[test]
fn sse_caps_the_number_of_live_sessions() {
    let server = Server::start_with("sse", &["--max-sessions", "2"]);
    let (first, _) = server.open_sse();
    let (_second, _) = server.open_sse();
    let third = server.send("GET", "/sse", &[], "");
    assert_eq!(third.status, 503);
    assert!(third.body().contains("session limit"));
    // Hanging up frees the slot.
    drop(first);
    let status = eventually(200, || server.send("GET", "/sse", &[], "").status);
    assert_eq!(status, 200, "the slot was not freed");
}
