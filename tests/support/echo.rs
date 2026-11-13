// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! A server with one tool, `echo`, for the tests to serve.
//!
//! Shared by the unit tests (`src/lib.rs` includes it) and the
//! integration tests. It depends on the SDK only, so it compiles the
//! same in both.

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};
use std::future::{Future, ready};
use std::sync::Arc;

/// The name the server introduces itself by.
pub(crate) const NAME: &str = "echo-mcp";

/// Answers `echo` with its `text` argument.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Echo;

impl Echo {
    fn tool() -> Tool {
        let schema = json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text string echoed back verbatim.",
                    "minLength": 1
                }
            },
            "required": ["text"],
            "additionalProperties": false
        });
        let Value::Object(schema) = schema else {
            unreachable!("a JSON object literal")
        };
        let output_schema = json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Echoed text."
                }
            },
            "required": ["text"],
            "additionalProperties": false
        });
        let Value::Object(output_schema) = output_schema else {
            unreachable!("a JSON object literal")
        };
        Tool::new("echo", "Answer with the text sent", schema)
            .with_title("Echo text")
            .with_annotations(
                ToolAnnotations::new()
                    .read_only(true)
                    .destructive(false)
                    .idempotent(true)
                    .open_world(false),
            )
            .with_raw_output_schema(Arc::new(output_schema))
    }
}

impl ServerHandler for Echo {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(NAME, "0.0.1"))
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

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>>
    + MaybeSendFuture
    + '_ {
        ready(Ok(Self::answer(&request).into()))
    }
}

impl Echo {
    /// The result of one call.
    fn answer(request: &CallToolRequestParams) -> CallToolResult {
        let text = request
            .arguments
            .as_ref()
            .and_then(|a| a.get("text"))
            .and_then(Value::as_str);
        match (request.name.as_ref(), text) {
            ("echo", Some(text)) => {
                let mut res =
                    CallToolResult::success(vec![ContentBlock::text(text)]);
                res.structured_content = Some(json!({ "text": text }));
                res
            }
            ("echo", None) => CallToolResult::error(vec![ContentBlock::text(
                "echo needs a `text` argument",
            )]),
            (other, _) => CallToolResult::error(vec![ContentBlock::text(
                format!("Unknown tool: {other}"),
            )]),
        }
    }
}
