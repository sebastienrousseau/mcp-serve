// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! An in-process server and a literal HTTP/1.1 client.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde_json::{Value, json};

use crate::echo::Echo;

/// `Accept` for a streamable HTTP POST.
pub(crate) const ACCEPT_BOTH: (&str, &str) =
    ("Accept", "application/json, text/event-stream");
/// The content type every client message carries.
pub(crate) const JSON: (&str, &str) = ("Content-Type", "application/json");
/// The header naming the stateless revision.
pub(crate) const STATELESS: (&str, &str) =
    ("MCP-Protocol-Version", "2026-07-28");
/// What a page that rebound its own name to 127.0.0.1 sends.
pub(crate) const EVIL_HOST: (&str, &str) = ("Host", "evil.example");
/// What a page on another origin sends.
pub(crate) const EVIL_ORIGIN: (&str, &str) = ("Origin", "http://evil.example");
/// The smallest request.
pub(crate) const PING: &str = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

/// An `initialize` request for `version`.
pub(crate) fn initialize(version: &str) -> String {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": version, "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}})
    .to_string()
}

/// A request in the stateless revision, its metadata in `_meta`.
pub(crate) fn stateless(id: u32, method: &str, params: Value) -> String {
    let mut params = params;
    // Both keys are required by the revision; a request naming only
    // its version is refused.
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        .to_string()
}

/// A free port below every common ephemeral range (Linux starts at
/// 32768, macOS at 49152), so a port the system hands out for `:0`
/// elsewhere cannot collide with it.
fn free_port_outside_ephemeral_range() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let seed = u16::try_from(std::process::id() % 10_000).unwrap_or(0);
    for _ in 0..10_000 {
        let offset = NEXT.fetch_add(1, Ordering::Relaxed);
        let port = 20_000 + (seed + offset) % 10_000;
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    panic!("no free port in 20000..30000");
}

/// A server running in-process on its own runtime, stopped when
/// dropped.
pub(crate) struct Server {
    pub(crate) addr: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Held while a server picks a port and starts listening, so two
/// tests starting at once cannot be handed the same free port.
static STARTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Server {
    /// Serve `transport` with the default options.
    pub(crate) fn start(transport: &str) -> Self {
        Self::start_with(transport, &[])
    }

    /// Start with `extra` arguments after the transport and port.
    pub(crate) fn start_with(transport: &str, extra: &[&str]) -> Self {
        let _starting = STARTING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Another process can still take the port between release and
        // bind; a failed bind is reported back and retried.
        for _ in 0..5 {
            if let Some(server) = Self::try_start(transport, extra) {
                return server;
            }
        }
        panic!("no free port for the {transport} server after 5 tries");
    }

    /// Serve on a port just found free. `None` if the bind failed.
    fn try_start(transport: &str, extra: &[&str]) -> Option<Self> {
        let port = free_port_outside_ephemeral_range().to_string();
        let args = ["--transport", transport, "--port", &port]
            .into_iter()
            .chain(extra.iter().copied())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mcp_serve::Command::Serve(options, _) =
            mcp_serve::parse(args).expect("valid arguments")
        else {
            panic!("expected a serve command");
        };
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let (failed_tx, failed) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("runtime");
            runtime.block_on(async move {
                tokio::select! {
                    served = mcp_serve::serve(&options, || Echo) => {
                        let _ = failed_tx.send(served);
                    }
                    _ = stopped => {}
                }
            });
        });
        let addr = format!("127.0.0.1:{port}");
        for _ in 0..100 {
            if let Ok(result) = failed.try_recv() {
                let _ = thread.join();
                assert!(result.is_err(), "server stopped on its own");
                return None;
            }
            if answers_http(&addr) {
                let (stop, thread) = (Some(stop), Some(thread));
                return Some(Self { addr, stop, thread });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("server did not start listening on {addr}");
    }

    /// One request to this server.
    pub(crate) fn send(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Http {
        Http::send(&self.addr, method, path, headers, body)
    }

    /// A streamable HTTP `POST /mcp`.
    pub(crate) fn post_mcp(
        &self,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Http {
        self.send("POST", "/mcp", headers, body)
    }

    /// `GET /sse`: a new legacy session, and the endpoint it posts to.
    pub(crate) fn open_sse(&self) -> (Http, String) {
        let mut stream = self.send("GET", "/sse", &[], "");
        assert_eq!(stream.status, 200);
        let (event, endpoint) = stream.next_event();
        assert_eq!(event, "endpoint");
        (stream, endpoint)
    }
}

/// True once something on `addr` answers an HTTP request. A bare TCP
/// connect is not enough: another process can hold a listening socket
/// on the same port and accept the handshake without answering.
fn answers_http(addr: &str) -> bool {
    let Ok(mut stream) = TcpStream::connect(addr) else {
        return false;
    };
    let probe = format!(
        "GET /ready HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    );
    let mut head = [0_u8; 9];
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .is_ok()
        && stream.write_all(probe.as_bytes()).is_ok()
        && stream.read_exact(&mut head).is_ok()
        && head.starts_with(b"HTTP/1.")
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One HTTP/1.1 exchange, with the body read incrementally so an
/// event stream can be consumed as it arrives.
pub(crate) struct Http {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    reader: BufReader<TcpStream>,
    chunked: bool,
    remaining: Option<usize>,
    /// Decoded body bytes not yet handed out.
    pending: Vec<u8>,
}

/// The request text. A caller-supplied `Host` replaces the default,
/// which is how a test plays a page that reached the server through
/// DNS rebinding.
fn request(
    addr: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let is_host = |n: &str| n.eq_ignore_ascii_case("host");
    let host = headers
        .iter()
        .find(|(n, _)| is_host(n))
        .map_or(addr, |(_, v)| *v);
    let mut text = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n"
    );
    for (name, value) in headers.iter().filter(|(n, _)| !is_host(n)) {
        let _ = write!(text, "{name}: {value}\r\n");
    }
    let _ = write!(text, "Content-Length: {}\r\n\r\n{body}", body.len());
    text
}

/// The status and the lowercased headers of a response.
fn read_head(
    reader: &mut BufReader<TcpStream>,
) -> (u16, Vec<(String, String)>) {
    let mut line = String::new();
    let _ = reader.read_line(&mut line).expect("status line");
    let status: u16 = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("bad status line {line:?}"));
    let mut headers = Vec::new();
    loop {
        line.clear();
        let _ = reader.read_line(&mut line).expect("header");
        let Some((name, value)) = line.trim_end().split_once(':') else {
            break;
        };
        headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
    }
    (status, headers)
}

impl Http {
    fn send(
        addr: &str,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Self {
        let mut stream = TcpStream::connect(addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        let text = request(addr, method, path, headers, body);
        stream.write_all(text.as_bytes()).expect("write");
        let mut reader = BufReader::new(stream);
        let (status, headers) = read_head(&mut reader);
        let mut http = Self {
            status,
            headers,
            reader,
            chunked: false,
            remaining: None,
            pending: Vec::new(),
        };
        http.chunked = http
            .header("transfer-encoding")
            .is_some_and(|v| v.contains("chunked"));
        http.remaining =
            http.header("content-length").and_then(|v| v.parse().ok());
        http
    }

    /// The value of the response header `name` (lowercase).
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// True when the response is an event stream.
    pub(crate) fn is_event_stream(&self) -> bool {
        self.header("content-type")
            .is_some_and(|c| c.starts_with("text/event-stream"))
    }

    /// Pull the next piece of body into `pending`. False at the end.
    fn fill(&mut self) -> bool {
        if self.chunked {
            self.fill_chunk()
        } else {
            self.fill_plain()
        }
    }

    fn fill_chunk(&mut self) -> bool {
        let mut size = String::new();
        let _ = self.reader.read_line(&mut size).expect("chunk size");
        let size = usize::from_str_radix(size.trim(), 16).expect("hex size");
        if size == 0 {
            return false;
        }
        let mut chunk = vec![0; size + 2];
        self.reader.read_exact(&mut chunk).expect("chunk");
        chunk.truncate(size);
        self.pending.extend_from_slice(&chunk);
        true
    }

    fn fill_plain(&mut self) -> bool {
        let want = self.remaining.unwrap_or(usize::MAX).min(4096);
        if want == 0 {
            return false;
        }
        let mut buf = vec![0; want];
        let n = self.reader.read(&mut buf).expect("read");
        if n == 0 {
            return false;
        }
        self.pending.extend_from_slice(&buf[..n]);
        if let Some(r) = self.remaining.as_mut() {
            *r -= n;
        }
        true
    }

    /// The whole body, for a response that ends.
    pub(crate) fn body(mut self) -> String {
        while self.fill() {}
        String::from_utf8(self.pending).expect("utf-8")
    }

    /// The next `event` with a `data` field, as (event name, data).
    ///
    /// Keep-alive comments and priming events carry no data and are
    /// skipped.
    pub(crate) fn next_event(&mut self) -> (String, String) {
        loop {
            if let Some(event) = self.take_event() {
                if !event.1.is_empty() {
                    return event;
                }
                continue;
            }
            assert!(self.fill(), "stream ended before an event arrived");
        }
    }

    /// One complete event block out of `pending`, if one has arrived.
    fn take_event(&mut self) -> Option<(String, String)> {
        let text = String::from_utf8_lossy(&self.pending).into_owned();
        let end = text.find("\n\n")?;
        let _ = self.pending.drain(..end + 2);
        let mut event = "message".to_owned();
        let mut data = Vec::new();
        for line in text[..end].lines() {
            if let Some(v) = line.strip_prefix("event:") {
                v.trim().clone_into(&mut event);
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push(v.trim().to_owned());
            }
        }
        Some((event, data.join("\n")))
    }

    /// The next JSON-RPC message on the stream.
    pub(crate) fn next_message(&mut self) -> Value {
        let (event, data) = self.next_event();
        assert_eq!(event, "message", "unexpected event {event}: {data}");
        serde_json::from_str(&data).unwrap_or_else(|e| panic!("{e}: {data}"))
    }
}

/// Retry `attempt` until it returns `want`, for a state change the
/// server makes in the background (a session ending). The last status.
pub(crate) fn eventually(want: u16, mut attempt: impl FnMut() -> u16) -> u16 {
    let mut status = 0;
    for _ in 0..50 {
        status = attempt();
        if status == want {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    status
}
