// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! The command line. Ported from the `transport/tests.rs` of
//! noyalib-mcp, oxml-mcp and rlg-mcp; the `--root` and `--profile`
//! cases from noyalib-mcp now run against a stand-in for its flags.

use super::*;

/// What `parse` returns for a command line that serves `options`.
fn serving(options: Options) -> Command {
    Command::Serve(options, NoFlags)
}

#[test]
fn no_arguments_means_stdio() {
    assert_eq!(parse(Vec::<String>::new()), Ok(serving(Options::default())));
}

#[test]
fn the_http_transports_take_host_and_port() {
    let want = Options {
        transport: Transport::StreamableHttp,
        host: "0.0.0.0".to_owned(),
        port: 9000,
        max_sessions: DEFAULT_MAX_SESSIONS,
    };
    assert_eq!(
        parse([
            "--transport",
            "streamable-http",
            "--host",
            "0.0.0.0",
            "--port",
            "9000"
        ]),
        Ok(serving(want.clone()))
    );
    // `--flag=value` is the same as `--flag value`.
    assert_eq!(
        parse([
            "--transport=streamable-http",
            "--host=0.0.0.0",
            "--port=9000"
        ]),
        Ok(serving(want))
    );
    assert_eq!(
        parse(["--transport", "sse"]),
        Ok(serving(Options {
            transport: Transport::Sse,
            ..Options::default()
        }))
    );
    assert_eq!(
        parse(["--transport", "stdio"]),
        Ok(serving(Options::default()))
    );
}

#[test]
fn help_and_version_win_over_everything_else() {
    assert_eq!(parse(["--help"]), Ok(Command::Help));
    assert_eq!(parse(["-h"]), Ok(Command::Help));
    assert_eq!(parse(["--version"]), Ok(Command::Version));
    assert_eq!(parse(["--transport", "sse", "-V"]), Ok(Command::Version));
}

#[test]
fn bad_arguments_are_named() {
    assert!(
        parse(["--transport", "carrier-pigeon"])
            .is_err_and(|e| e.contains("carrier-pigeon"))
    );
    assert!(parse(["--port", "eighty"]).is_err_and(|e| e.contains("eighty")));
    assert!(parse(["--port", "70000"]).is_err_and(|e| e.contains("70000")));
    assert!(parse(["--port"]).is_err_and(|e| e.contains("needs a value")));
    assert!(parse(["--bogus"]).is_err_and(|e| e.contains("--bogus")));
}

#[test]
fn max_sessions_takes_a_positive_count() {
    assert_eq!(Options::default().max_sessions, DEFAULT_MAX_SESSIONS);
    assert_eq!(
        parse(["--max-sessions", "3"]),
        Ok(serving(Options {
            max_sessions: 3,
            ..Options::default()
        }))
    );
    assert!(parse(["--max-sessions=3"]).is_ok_and(|c| matches!(
        c,
        Command::Serve(
            Options {
                max_sessions: 3,
                ..
            },
            _
        )
    )));
    for bad in ["0", "-1", "many"] {
        assert!(
            parse(["--max-sessions", bad]).is_err_and(
                |e| e.contains(bad) && e.contains("positive number")
            ),
            "{bad}"
        );
    }
}

#[test]
fn flags_split_on_the_first_equals_and_unknown_ones_stop_parsing() {
    // An unknown flag fails before anything after it is read, even a
    // flag that would otherwise win.
    assert!(parse(["--bogus", "--help"]).is_err_and(|e| e.contains("--bogus")));
    assert!(parse(["--bogus=1"]).is_err_and(|e| e.contains("`--bogus`")));
    // Only the first `=` separates the value.
    match parse(["--transport=sse", "--host=a=b", "--port=0"]) {
        Ok(Command::Serve(options, NoFlags)) => {
            assert_eq!(options.transport, Transport::Sse);
            assert_eq!(options.host, "a=b");
            assert_eq!(options.port, 0);
        }
        other => panic!("expected serve, got {other:?}"),
    }
    // A value-taking flag reads the next argument as its value.
    assert!(parse(["--host", "--help"]).is_ok_and(|c| matches!(
        c,
        Command::Serve(Options { ref host, .. }, _) if host == "--help"
    )));
}

#[test]
fn the_usage_text_names_every_flag_and_path() {
    let text = usage("any-mcp");
    for needle in [
        "any-mcp",
        "--transport",
        "--host",
        "--port",
        "--max-sessions",
        "--version",
        "--help",
        STREAMABLE_HTTP_PATH,
        SSE_PATH,
        MESSAGE_PATH,
    ] {
        assert!(text.contains(needle), "usage lacks {needle}:\n{text}");
    }
    // No server flags: the usage line ends with the shared ones.
    assert!(text.contains("[--max-sessions <n>]\n"), "{text}");
}

#[test]
fn transport_names_round_trip() {
    for (name, transport) in [
        ("stdio", Transport::Stdio),
        ("streamable-http", Transport::StreamableHttp),
        ("sse", Transport::Sse),
    ] {
        assert_eq!(Transport::from_name(name), Some(transport));
    }
    assert_eq!(Transport::from_name("SSE"), None);
}

#[test]
fn no_flags_refuses_any_flag_handed_to_it() {
    let mut flags = NoFlags;
    assert!(flags.set("--root", "x".to_owned()).is_err());
    assert!(flags.validate().is_ok());
}

// --- A server's own flags --------------------------------------------------

/// A stand-in for noyalib-mcp's `--root` and `--profile`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Yaml {
    root: Option<String>,
    strict: bool,
    validated: bool,
}

impl ServerFlags for Yaml {
    const NAMES: &'static [&'static str] = &["--root", "--profile"];
    const SYNOPSIS: &'static str =
        "[--root <dir>] [--profile <strict|standard>]";
    const HELP: &'static str = "\x20 --root <dir>        directory the \
                                file tools may read and write\n\
                                \x20 --profile <name>    strict or \
                                standard\n";

    fn set(&mut self, flag: &str, value: String) -> Result<(), String> {
        match flag {
            "--root" => self.root = Some(value),
            _ => {
                self.strict = match value.as_str() {
                    "strict" => true,
                    "standard" => false,
                    other => {
                        return Err(format!("unknown profile `{other}`"));
                    }
                };
            }
        }
        Ok(())
    }

    fn validate(&mut self) -> Result<(), String> {
        if self.root.as_deref() == Some("/missing") {
            return Err("--root /missing: not a directory".to_owned());
        }
        self.validated = true;
        Ok(())
    }
}

#[test]
fn server_flags_are_read_alongside_the_shared_ones() {
    let parsed = parse_with::<Yaml, _>([
        "--root",
        "/srv/yaml",
        "--transport=sse",
        "--profile=strict",
    ]);
    let want = Yaml {
        root: Some("/srv/yaml".to_owned()),
        strict: true,
        validated: false,
    };
    assert_eq!(
        parsed,
        Ok(Command::Serve(
            Options {
                transport: Transport::Sse,
                ..Options::default()
            },
            want
        ))
    );
}

#[test]
fn server_flags_fail_like_shared_ones() {
    assert!(
        parse_with::<Yaml, _>(["--root"])
            .is_err_and(|e| e.contains("--root needs a value"))
    );
    assert!(
        parse_with::<Yaml, _>(["--profile", "lax"])
            .is_err_and(|e| e.contains("lax"))
    );
    // A flag is only known to the server that declares it.
    assert!(parse(["--root", "/srv"]).is_err_and(|e| e.contains("--root")));
    assert!(
        parse_with::<Yaml, _>(["--bogus"])
            .is_err_and(|e| e.contains("--bogus"))
    );
}

#[test]
fn server_flags_appear_in_the_usage_text() {
    let text = usage_with::<Yaml>("noyalib-mcp");
    assert!(
        text.contains(
            "[--max-sessions <n>] [--root <dir>] \
             [--profile <strict|standard>]\n"
        ),
        "{text}"
    );
    let root = text.find("  --root <dir>").expect("root line");
    let sessions = text.find("  --max-sessions <n>").expect("shared");
    let version = text.find("  --version").expect("version line");
    assert!(sessions < root && root < version, "{text}");
}

/// A server that tries to take over a shared flag.
#[derive(Debug, Default, PartialEq, Eq)]
struct Greedy(Option<String>);

impl ServerFlags for Greedy {
    const NAMES: &'static [&'static str] = &["--port"];
    const SYNOPSIS: &'static str = "";
    const HELP: &'static str = "";

    fn set(&mut self, _flag: &str, value: String) -> Result<(), String> {
        self.0 = Some(value);
        Ok(())
    }
}

#[test]
fn a_shared_flag_keeps_its_shared_meaning() {
    let Ok(Command::Serve(options, greedy)) =
        parse_with::<Greedy, _>(["--port", "9"])
    else {
        panic!("expected serve");
    };
    assert_eq!(options.port, 9);
    assert_eq!(greedy, Greedy(None));
}

// --- Switches: server flags that take no value ------------------------------

/// A server with one value-taking flag and two switches, one of which
/// tries to take over a shared flag.
#[derive(Debug, Default, PartialEq, Eq)]
struct Switchy {
    root: Option<String>,
    dry_run: bool,
    verbose: bool,
}

impl ServerFlags for Switchy {
    const NAMES: &'static [&'static str] = &["--root"];
    const SWITCHES: &'static [&'static str] =
        &["--dry-run", "--verbose", "--host"];
    const SYNOPSIS: &'static str = "[--root <dir>] [--dry-run] [--verbose]";
    const HELP: &'static str = "\x20 --root <dir>        a directory\n\
                                \x20 --dry-run           change nothing\n\
                                \x20 --verbose           say more\n";

    fn set(&mut self, _flag: &str, value: String) -> Result<(), String> {
        self.root = Some(value);
        Ok(())
    }

    fn set_switch(&mut self, flag: &str) -> Result<(), String> {
        match flag {
            "--dry-run" => self.dry_run = true,
            "--verbose" => self.verbose = true,
            other => return Err(format!("not a switch: {other}")),
        }
        Ok(())
    }
}

#[test]
fn a_switch_takes_no_value_and_leaves_the_next_argument_alone() {
    let parsed =
        parse_with::<Switchy, _>(["--dry-run", "--root", "/srv", "--verbose"]);
    let want = Switchy {
        root: Some("/srv".to_owned()),
        dry_run: true,
        verbose: true,
    };
    assert_eq!(parsed, Ok(Command::Serve(Options::default(), want)));
    // The argument after a switch is read as a flag of its own.
    assert_eq!(
        parse_with::<Switchy, _>(["--dry-run", "--help"]),
        Ok(Command::Help)
    );
    assert!(
        parse_with::<Switchy, _>(["--dry-run", "yes"])
            .is_err_and(|e| e.contains("`yes`"))
    );
}

#[test]
fn a_switch_given_a_value_is_a_usage_error() {
    for arg in ["--dry-run=yes", "--dry-run=", "--verbose=true"] {
        assert!(
            parse_with::<Switchy, _>([arg])
                .is_err_and(|e| e.contains("takes no value")),
            "{arg}"
        );
    }
}

#[test]
fn a_switch_is_only_known_to_the_server_that_declares_it() {
    assert!(parse(["--dry-run"]).is_err_and(|e| e.contains("--dry-run")));
    assert!(
        parse_with::<Yaml, _>(["--dry-run"])
            .is_err_and(|e| e.contains("--dry-run"))
    );
    let mut flags = NoFlags;
    assert!(flags.set_switch("--dry-run").is_err());
}

#[test]
fn a_shared_flag_listed_as_a_switch_keeps_its_value() {
    let Ok(Command::Serve(options, flags)) =
        parse_with::<Switchy, _>(["--host", "::1"])
    else {
        panic!("expected serve");
    };
    assert_eq!(options.host, "::1");
    assert_eq!(flags, Switchy::default());
}
