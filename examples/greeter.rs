// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! A whole MCP server in one file: one tool, `greet`, and one flag of
//! its own, `--greeting`.
//!
//! ```text
//! cargo run --example greeter -- --help
//! cargo run --example greeter -- --greeting Bonjour
//! cargo run --example greeter -- --transport streamable-http --port 8000
//! ```
//!
//! The tool answers through [`mcp_serve::reply`] (text and structured
//! content) and runs its work through [`mcp_serve::run_blocking`], the
//! opt-in per-call time limit.

use std::fmt;
use std::future::{Future, ready};
use std::process::ExitCode;
use std::sync::Arc;

use mcp_serve::rmcp::model::{
    CallToolRequestParams, CallToolResponse, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use mcp_serve::rmcp::service::{MaybeSendFuture, RequestContext};
use mcp_serve::rmcp::{ErrorData, RoleServer, ServerHandler};
use mcp_serve::{DEFAULT_CALL_TIMEOUT, ServerFlags};
use serde::Serialize;
use serde_json::{Value, json};

/// The server's own flags.
#[derive(Debug)]
struct Flags {
    greeting: String,
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            greeting: "Hello".to_owned(),
        }
    }
}

impl ServerFlags for Flags {
    const NAMES: &'static [&'static str] = &["--greeting"];
    const SYNOPSIS: &'static str = "[--greeting <text>]";
    const HELP: &'static str =
        "\x20 --greeting <text>   how `greet` begins (default Hello)\n";

    fn set(&mut self, _flag: &str, value: String) -> Result<(), String> {
        value.clone_into(&mut self.greeting);
        Ok(())
    }

    fn validate(&mut self) -> Result<(), String> {
        if self.greeting.trim().is_empty() {
            return Err("--greeting may not be blank".to_owned());
        }
        Ok(())
    }
}

/// What `greet` answers.
#[derive(Debug, Serialize)]
struct Greeting {
    text: String,
}

impl fmt::Display for Greeting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// The server: one per stdio session, one per HTTP session.
#[derive(Debug, Clone)]
struct Greeter {
    greeting: String,
}

impl Greeter {
    fn tool() -> Tool {
        let Value::Object(schema) = json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the person or caller to greet.",
                    "minLength": 1,
                    "examples": ["Ada"]
                }
            },
            "required": ["name"],
            "additionalProperties": false
        }) else {
            unreachable!("a JSON object literal")
        };
        let Value::Object(output_schema) = json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Rendered greeting text."
                }
            },
            "required": ["text"],
            "additionalProperties": false
        }) else {
            unreachable!("a JSON object literal")
        };
        Tool::new("greet", "Greet someone by name", schema)
            .with_title("Greet by name")
            .with_annotations(
                ToolAnnotations::new()
                    .read_only(true)
                    .destructive(false)
                    .idempotent(true)
                    .open_world(false),
            )
            .with_raw_output_schema(Arc::new(output_schema))
    }

    fn greet(
        &self,
        request: &CallToolRequestParams,
    ) -> Result<Greeting, String> {
        if request.name != "greet" {
            return Err(format!(
                "unknown tool `{}`; only `greet` is available",
                request.name
            ));
        }
        let name = request
            .arguments
            .as_ref()
            .and_then(|a| a.get("name"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or("missing required non-empty `name` string argument")?;
        Ok(Greeting {
            text: format!("{}, {name}!", self.greeting),
        })
    }
}

impl ServerHandler for Greeter {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "greeter",
                env!("CARGO_PKG_VERSION"),
            ))
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>>
    + MaybeSendFuture
    + '_ {
        ready(Ok(ListToolsResult::with_all_items(vec![Self::tool()])))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let server = self.clone();
        let outcome =
            mcp_serve::run_blocking(DEFAULT_CALL_TIMEOUT, move || {
                server.greet(&request)
            })
            .await;
        mcp_serve::reply(outcome, |_| false).map(Into::into)
    }
}

fn main() -> ExitCode {
    mcp_serve::serve_main_with(
        "greeter",
        env!("CARGO_PKG_VERSION"),
        std::env::args().skip(1),
        |flags: &Flags| Greeter {
            greeting: flags.greeting.clone(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_declares_parameter_docs_annotations_and_output_schema() {
        let tool = Greeter::tool();
        let schema = tool.schema_as_json_value();
        let name_prop = &schema["properties"]["name"];
        assert!(
            name_prop["description"]
                .as_str()
                .is_some_and(|d| !d.trim().is_empty()),
            "greet.name must be described: {schema}"
        );
        assert_eq!(name_prop["minLength"], 1);

        let ann = tool.annotations.expect("greet must carry annotations");
        assert_eq!(ann.read_only_hint, Some(true));
        assert_eq!(ann.destructive_hint, Some(false));
        assert_eq!(ann.idempotent_hint, Some(true));
        assert_eq!(ann.open_world_hint, Some(false));

        let out = tool
            .output_schema
            .as_ref()
            .expect("greet must declare outputSchema");
        let out_json = serde_json::to_value(out.as_ref()).expect("json");
        assert_eq!(out_json["properties"]["text"]["type"], "string");
    }

    #[test]
    fn greet_validates_tool_name_and_emits_structured_content() {
        let server = Greeter {
            greeting: "Bonjour".to_owned(),
        };
        let mut args = serde_json::Map::new();
        let _ = args.insert("name".to_owned(), Value::from("Ada"));
        let ok = mcp_serve::reply(
            server.greet(
                &CallToolRequestParams::new("greet")
                    .with_arguments(args.clone()),
            ),
            |_| false,
        )
        .expect("serialises");
        assert_eq!(ok.is_error, Some(false));
        assert_eq!(
            ok.structured_content,
            Some(json!({"text": "Bonjour, Ada!"}))
        );

        let unknown = server
            .greet(&CallToolRequestParams::new("nope").with_arguments(args));
        assert!(unknown.is_err(), "unknown tool should be rejected");
    }
}
