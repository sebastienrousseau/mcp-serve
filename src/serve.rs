// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! Serving a handler: the process entry point and the three
//! transports.

use std::io;
use std::process::ExitCode;

use axum::Router;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ServerHandler, ServiceExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::bounded_lines::BoundedLines;
use crate::cli::{
    Command, NoFlags, Options, SSE_PATH, STREAMABLE_HTTP_PATH, ServerFlags,
    Transport, parse_with, usage_with,
};
use crate::guard;
use crate::sessions::CappedSessions;
use crate::sse::serve_sse;

/// Serve `factory`'s handler as `name` according to the command line.
///
/// This is the whole of a server's `main`: read the command line,
/// serve, and turn the outcome into an exit status. `--help` and
/// `--version` print to stdout and exit with 0; a usage error prints
/// the reason and the usage text to stderr and exits with 2; a
/// transport failure prints the reason and exits with 1. A stdio
/// session the client ends is a success.
///
/// `args` excludes the program name. `factory` is called once for the
/// stdio session and once per session over HTTP.
///
/// # Examples
///
/// ```no_run
/// use rmcp::ServerHandler;
///
/// #[derive(Clone)]
/// struct Server;
/// impl ServerHandler for Server {}
///
/// fn main() -> std::process::ExitCode {
///     mcp_serve::serve_main(
///         "my-mcp",
///         env!("CARGO_PKG_VERSION"),
///         std::env::args().skip(1),
///         || Server,
///     )
/// }
/// ```
pub fn serve_main<H, F, I>(
    name: &str,
    version: &str,
    args: I,
    factory: F,
) -> ExitCode
where
    H: ServerHandler,
    F: Fn() -> H + Send + Sync + 'static,
    I: IntoIterator,
    I::Item: Into<String>,
{
    serve_main_with(name, version, args, move |_: &NoFlags| factory())
}

/// [`serve_main`] for a server with flags of its own.
///
/// The command line is read with [`parse_with`]; once it parses,
/// [`ServerFlags::validate`] runs, and an error there is a usage error
/// (status 2) reported before anything is served. `factory` receives
/// the validated flags each time it makes a handler.
///
/// # Examples
///
/// ```no_run
/// use mcp_serve::ServerFlags;
/// use rmcp::ServerHandler;
///
/// #[derive(Clone)]
/// struct Server(String);
/// impl ServerHandler for Server {}
///
/// #[derive(Default)]
/// struct Flags(String);
/// impl ServerFlags for Flags {
///     const NAMES: &'static [&'static str] = &["--greeting"];
///     const SYNOPSIS: &'static str = "[--greeting <text>]";
///     const HELP: &'static str =
///         "  --greeting <text>   what the server says first\n";
///     fn set(&mut self, _: &str, value: String) -> Result<(), String> {
///         self.0 = value;
///         Ok(())
///     }
/// }
///
/// fn main() -> std::process::ExitCode {
///     mcp_serve::serve_main_with(
///         "my-mcp",
///         env!("CARGO_PKG_VERSION"),
///         std::env::args().skip(1),
///         |flags: &Flags| Server(flags.0.clone()),
///     )
/// }
/// ```
pub fn serve_main_with<X, H, F, I>(
    name: &str,
    version: &str,
    args: I,
    factory: F,
) -> ExitCode
where
    X: ServerFlags,
    H: ServerHandler,
    F: Fn(&X) -> H + Send + Sync + 'static,
    I: IntoIterator,
    I::Item: Into<String>,
{
    ExitCode::from(exit_status(name, version, args, factory))
}

/// The exit status [`serve_main_with`] ends with.
pub(crate) fn exit_status<X, H, F, I>(
    name: &str,
    version: &str,
    args: I,
    factory: F,
) -> u8
where
    X: ServerFlags,
    H: ServerHandler,
    F: Fn(&X) -> H + Send + Sync + 'static,
    I: IntoIterator,
    I::Item: Into<String>,
{
    let (options, flags) = match ready_to_serve::<X, I>(name, version, args) {
        Ok(ready) => ready,
        Err(Early::Print(text)) => {
            println!("{text}");
            return 0;
        }
        Err(Early::Usage(text)) => {
            eprintln!("{text}");
            return 2;
        }
    };
    let make = move || factory(&flags);
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("{name}: cannot start the async runtime: {e}");
            return 1;
        }
    };
    match runtime.block_on(serve(&options, make)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{name}: {e}");
            1
        }
    }
}

/// Why the process ends before serving.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Early {
    /// `--help` or `--version`: print this to stdout and exit with 0.
    Print(String),
    /// A usage error: print this to stderr and exit with 2.
    Usage(String),
}

/// The options and flags to serve with, or why the process ends
/// first: the command line asked for help or the version, did not
/// parse, or named values the server cannot use.
pub(crate) fn ready_to_serve<X, I>(
    name: &str,
    version: &str,
    args: I,
) -> Result<(Options, X), Early>
where
    X: ServerFlags,
    I: IntoIterator,
    I::Item: Into<String>,
{
    let usage_error = |message: &str| {
        Early::Usage(format!("{name}: {message}\n\n{}", usage_with::<X>(name)))
    };
    match parse_with::<X, I>(args) {
        Ok(Command::Serve(options, mut flags)) => match flags.validate() {
            Ok(()) => Ok((options, flags)),
            Err(message) => Err(usage_error(&message)),
        },
        Ok(Command::Help) => Err(Early::Print(usage_with::<X>(name))),
        Ok(Command::Version) => Err(Early::Print(format!("{name} {version}"))),
        Err(message) => Err(usage_error(&message)),
    }
}

/// Serve the handler over the chosen transport until the client goes
/// away (stdio) or the process is interrupted (HTTP).
///
/// For a server that builds its own runtime or reads its own command
/// line; [`serve_main`] calls this.
///
/// # Errors
///
/// The listener could not bind, or the transport failed underneath the
/// session.
pub async fn serve<H, F>(options: &Options, factory: F) -> io::Result<()>
where
    H: ServerHandler,
    F: Fn() -> H + Send + Sync + 'static,
{
    match options.transport {
        Transport::Stdio => {
            serve_io(factory(), tokio::io::stdin(), tokio::io::stdout()).await
        }
        Transport::StreamableHttp => {
            let listener = bind(options).await?;
            announce(&listener, STREAMABLE_HTTP_PATH)?;
            serve_streamable_http(listener, options, factory).await
        }
        Transport::Sse => {
            let listener = bind(options).await?;
            announce(&listener, SSE_PATH)?;
            serve_sse(listener, options, factory).await
        }
    }
}

/// One stdio-framed session over `input` and `output`.
///
/// The SDK's own newline framing, over a reader that caps each line:
/// its framing would otherwise buffer an unended line until the pipe
/// closes.
pub(crate) async fn serve_io<H, R, W>(
    handler: H,
    input: R,
    output: W,
) -> io::Result<()>
where
    H: ServerHandler,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    use rmcp::service::ServerInitializeError as Init;
    let input = BoundedLines::new(input);
    let running = match handler.serve((input, output)).await {
        Ok(running) => running,
        // The client hung up, or spoke before the handshake, and there
        // is nobody left to report to: a session that never began is
        // the normal end of a stdio server, not a failure of it.
        Err(Init::ConnectionClosed(_) | Init::ExpectedInitializeRequest(_)) => {
            return Ok(());
        }
        Err(e) => return Err(io::Error::other(format!("stdio session: {e}"))),
    };
    // Likewise once it is under way: the client closing the pipe is
    // how a stdio session ends.
    let _ = running
        .waiting()
        .await
        .map_err(|e| io::Error::other(format!("stdio session: {e}")))?;
    Ok(())
}

async fn bind(options: &Options) -> io::Result<TcpListener> {
    TcpListener::bind((options.host.as_str(), options.port))
        .await
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "cannot listen on {}:{}: {e}",
                    options.host, options.port
                ),
            )
        })
}

/// Say where the server is, on stderr so stdout stays free.
///
/// A test starts the server on port 0 and reads the port from here.
fn announce(listener: &TcpListener, path: &str) -> io::Result<()> {
    let addr = listener.local_addr()?;
    eprintln!("listening on http://{addr}{path}");
    Ok(())
}

/// Resolve when the process is asked to stop.
pub(crate) async fn interrupted() {
    if tokio::signal::ctrl_c().await.is_err() {
        // No signal handler could be installed: stay up until killed.
        std::future::pending::<()>().await;
    }
}

async fn serve_streamable_http<H, F>(
    listener: TcpListener,
    options: &Options,
    factory: F,
) -> io::Result<()>
where
    H: ServerHandler,
    F: Fn() -> H + Send + Sync + 'static,
{
    let ct = CancellationToken::new();
    let mut config = StreamableHttpServerConfig::default()
        .with_cancellation_token(ct.child_token());
    // The SDK refuses a `Host` header it does not expect, which is
    // what stops a page in a browser from reaching a local server
    // through DNS rebinding. Loopback names are allowed by default;
    // an operator who binds another interface has chosen to be
    // reachable by it, so that name is allowed too. Binding every
    // interface means there is no name to check against.
    // A browser page also carries its own origin; only a local one is
    // served. A client that is not a browser sends no `Origin`.
    config = match guard::allowed_hosts(&options.host) {
        Some(hosts) => config.with_allowed_hosts(hosts),
        None => config.disable_allowed_hosts(),
    }
    .with_allowed_origins(guard::allowed_origins(&options.host));
    let service = StreamableHttpService::new(
        move || Ok(factory()),
        CappedSessions::new(options.max_sessions).into(),
        config,
    );
    let router = Router::new().nest_service(STREAMABLE_HTTP_PATH, service);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            interrupted().await;
            ct.cancel();
        })
        .await
}

#[cfg(test)]
mod tests;
