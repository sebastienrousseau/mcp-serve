// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! The command line every server shares, and the hook for its own
//! flags.

use std::fmt;

/// Where the listeners bind unless told otherwise.
pub const DEFAULT_HOST: &str = "127.0.0.1";
/// The port the HTTP transports listen on unless told otherwise.
pub const DEFAULT_PORT: u16 = 8000;
/// How many sessions an HTTP transport holds at once unless told
/// otherwise.
pub const DEFAULT_MAX_SESSIONS: usize = 64;
/// The streamable HTTP endpoint.
pub const STREAMABLE_HTTP_PATH: &str = "/mcp";
/// Where the legacy transport's event stream is opened.
pub const SSE_PATH: &str = "/sse";
/// Where the legacy transport's client messages are posted.
pub const MESSAGE_PATH: &str = "/messages/";

/// The flags this crate reads itself. A server's own flags may not
/// reuse these names: the shared meaning wins.
const SHARED_FLAGS: [&str; 4] =
    ["--transport", "--host", "--port", "--max-sessions"];

/// How the server talks to its client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// JSON-RPC over the process's own stdin and stdout.
    Stdio,
    /// The current HTTP transport, both protocol revisions.
    StreamableHttp,
    /// The 2024-11-05 HTTP+SSE transport.
    Sse,
}

impl Transport {
    /// The transport `--transport <name>` names, if any.
    ///
    /// ```
    /// use mcp_serve::Transport;
    /// assert_eq!(Transport::from_name("sse"), Some(Transport::Sse));
    /// assert_eq!(Transport::from_name("carrier-pigeon"), None);
    /// ```
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "stdio" => Some(Self::Stdio),
            "streamable-http" => Some(Self::StreamableHttp),
            "sse" => Some(Self::Sse),
            _ => None,
        }
    }
}

/// What the shared part of the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The transport to serve.
    pub transport: Transport,
    /// The interface the HTTP transports bind.
    pub host: String,
    /// The port the HTTP transports bind. Zero asks the system for a
    /// free one, which is what a test wants.
    pub port: u16,
    /// How many sessions an HTTP transport holds at once; past it, a
    /// new one is refused.
    pub max_sessions: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            transport: Transport::Stdio,
            host: DEFAULT_HOST.to_owned(),
            port: DEFAULT_PORT,
            max_sessions: DEFAULT_MAX_SESSIONS,
        }
    }
}

/// A server's own command-line flags, read alongside the shared ones.
///
/// Every flag named in [`NAMES`](Self::NAMES) takes one value, given
/// as `--flag value` or `--flag=value`, which is handed to
/// [`set`](Self::set). After the whole command line has been read,
/// [`validate`](Self::validate) may check or resolve the values (a
/// directory that must exist, say); an error there is a usage error,
/// reported before anything is served.
///
/// A server with no flags of its own uses [`NoFlags`].
///
/// # Examples
///
/// ```
/// use mcp_serve::{Command, ServerFlags, parse_with};
///
/// #[derive(Debug, Default, PartialEq)]
/// struct Flags {
///     root: Option<String>,
/// }
///
/// impl ServerFlags for Flags {
///     const NAMES: &'static [&'static str] = &["--root"];
///     const SYNOPSIS: &'static str = "[--root <dir>]";
///     const HELP: &'static str =
///         "  --root <dir>        directory the tools may use\n";
///
///     fn set(&mut self, _flag: &str, value: String) -> Result<(), String> {
///         self.root = Some(value);
///         Ok(())
///     }
/// }
///
/// let Ok(Command::Serve(_, flags)) =
///     parse_with::<Flags, _>(["--root=/srv"])
/// else {
///     panic!("expected a serve command");
/// };
/// assert_eq!(flags.root.as_deref(), Some("/srv"));
/// ```
pub trait ServerFlags: Default + Send + Sync + 'static {
    /// The flags this server adds, each spelt with its leading `--`.
    const NAMES: &'static [&'static str];
    /// What the flags add to the usage line, e.g.
    /// `[--root <dir>] [--profile <strict|standard>]`. Empty for none.
    const SYNOPSIS: &'static str;
    /// The option lines the flags add to `--help`, each ending in a
    /// newline and indented like the shared ones (two spaces, the
    /// description in column 23). Empty for none.
    const HELP: &'static str;

    /// Take `value` for `flag`, one of [`NAMES`](Self::NAMES).
    ///
    /// # Errors
    ///
    /// The value does not parse; the message is shown with the usage
    /// text.
    fn set(&mut self, flag: &str, value: String) -> Result<(), String>;

    /// Check or resolve the values once the command line is read.
    ///
    /// Runs only when the server is about to serve, never for `--help`
    /// or `--version`.
    ///
    /// # Errors
    ///
    /// The values cannot be served with; the message is shown with the
    /// usage text and the process exits with status 2.
    fn validate(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// The flags of a server that has none of its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoFlags;

impl ServerFlags for NoFlags {
    const NAMES: &'static [&'static str] = &[];
    const SYNOPSIS: &'static str = "";
    const HELP: &'static str = "";

    fn set(&mut self, flag: &str, _value: String) -> Result<(), String> {
        Err(unknown(flag))
    }
}

/// The outcome of reading the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command<X = NoFlags> {
    /// Serve with these options and the server's own flags.
    Serve(Options, X),
    /// `--help`: print the usage text and exit.
    Help,
    /// `--version`: print the version and exit.
    Version,
}

/// The usage text of a server with no flags of its own.
///
/// ```
/// let text = mcp_serve::usage("echo-mcp");
/// assert!(text.starts_with("Usage: echo-mcp [--transport"));
/// ```
#[must_use]
pub fn usage(name: &str) -> String {
    usage_with::<NoFlags>(name)
}

/// The usage text, for `--help` and for a usage error, with the
/// server's own flags from `X`.
#[must_use]
pub fn usage_with<X: ServerFlags>(name: &str) -> String {
    let synopsis = Synopsis(X::SYNOPSIS);
    let extra = X::HELP;
    format!(
        "Usage: {name} [--transport <stdio|streamable-http|sse>] \
         [--host <address>] [--port <number>] \
         [--max-sessions <n>]{synopsis}\n\
         \n\
         Options:\n\
         \x20 --transport <name>  stdio (default), streamable-http, or sse\n\
         \x20 --host <address>    interface for the HTTP transports \
         (default {DEFAULT_HOST})\n\
         \x20 --port <number>     port for the HTTP transports \
         (default {DEFAULT_PORT})\n\
         \x20 --max-sessions <n>  sessions an HTTP transport holds at \
         once (default {DEFAULT_MAX_SESSIONS})\n\
         {extra}\
         \x20 --version           print the version and exit\n\
         \x20 --help              print this text and exit\n\
         \n\
         streamable-http serves {STREAMABLE_HTTP_PATH} (MCP 2025-11-25 and \
         2026-07-28).\n\
         sse serves {SSE_PATH} and {MESSAGE_PATH} (MCP 2024-11-05).\n\
         Neither transport authenticates: keep them on the loopback \
         interface or\n\
         behind a gateway you trust."
    )
}

/// A server's synopsis, preceded by a space when there is one.
struct Synopsis(&'static str);

impl fmt::Display for Synopsis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            Ok(())
        } else {
            write!(f, " {}", self.0)
        }
    }
}

/// Read the command line of a server with no flags of its own.
///
/// `args` excludes the program name. Both `--flag value` and
/// `--flag=value` are accepted.
///
/// ```
/// use mcp_serve::{Command, Options, Transport, parse};
///
/// let Ok(Command::Serve(options, _)) =
///     parse(["--transport", "streamable-http", "--port=9000"])
/// else {
///     panic!("expected a serve command");
/// };
/// assert_eq!(options.transport, Transport::StreamableHttp);
/// assert_eq!(options.port, 9000);
/// ```
///
/// # Errors
///
/// A flag that is unknown, missing its value, or given a value that
/// does not parse. The message says which.
pub fn parse<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    parse_with::<NoFlags, I>(args)
}

/// Read the command line, with the server's own flags from `X`.
///
/// `--help` and `--version` win over everything after them; an
/// unknown flag fails before anything after it is read. A flag that
/// takes a value reads the next argument as that value, whatever it
/// looks like.
///
/// # Errors
///
/// A flag that is unknown, missing its value, or given a value that
/// does not parse. The message says which.
pub fn parse_with<X, I>(args: I) -> Result<Command<X>, String>
where
    X: ServerFlags,
    I: IntoIterator,
    I::Item: Into<String>,
{
    let mut options = Options::default();
    let mut flags = X::default();
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        let (flag, inline) = split_flag(arg);
        match flag.as_str() {
            "--help" | "-h" => return Ok(Command::Help),
            "--version" | "-V" => return Ok(Command::Version),
            other => {
                let value = flag_value::<X>(other, inline, &mut args)?;
                apply(&mut options, &mut flags, other, value)?;
            }
        }
    }
    Ok(Command::Serve(options, flags))
}

/// Split `--flag=value` at its first `=`; a bare `--flag` has no
/// inline value.
fn split_flag(arg: String) -> (String, Option<String>) {
    match arg.split_once('=') {
        Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
        None => (arg, None),
    }
}

/// The value of a known `flag`, from its inline value or else the next
/// argument. An unknown flag fails before any argument is read.
fn flag_value<X: ServerFlags>(
    flag: &str,
    inline: Option<String>,
    rest: &mut dyn Iterator<Item = String>,
) -> Result<String, String> {
    if !SHARED_FLAGS.contains(&flag) && !X::NAMES.contains(&flag) {
        return Err(unknown(flag));
    }
    inline
        .or_else(|| rest.next())
        .ok_or_else(|| format!("{flag} needs a value"))
}

/// Set the option a known `flag` names to `value`.
fn apply<X: ServerFlags>(
    options: &mut Options,
    flags: &mut X,
    flag: &str,
    value: String,
) -> Result<(), String> {
    match flag {
        "--host" => options.host = value,
        "--transport" => options.transport = parse_transport(&value)?,
        "--port" => options.port = parse_port(&value)?,
        "--max-sessions" => {
            options.max_sessions = parse_max_sessions(&value)?;
        }
        _ => flags.set(flag, value)?,
    }
    Ok(())
}

fn unknown(flag: &str) -> String {
    format!("unknown argument `{flag}`")
}

fn parse_transport(name: &str) -> Result<Transport, String> {
    Transport::from_name(name).ok_or_else(|| {
        format!(
            "unknown transport `{name}`; choose stdio, streamable-http or sse"
        )
    })
}

fn parse_max_sessions(text: &str) -> Result<usize, String> {
    text.parse()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("`{text}` is not a positive number of sessions"))
}

fn parse_port(text: &str) -> Result<u16, String> {
    text.parse()
        .map_err(|_| format!("`{text}` is not a port number"))
}

#[cfg(test)]
mod tests;
