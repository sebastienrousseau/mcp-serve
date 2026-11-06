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
    ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};
use std::future::{Future, ready};

/// The name the server introduces itself by.
pub(crate) const NAME: &str = "echo-mcp";

/// Answers `echo` with its `text` argument.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Echo;

impl Echo {
    fn tool() -> Tool {
        let schema = json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        });
        let Value::Object(schema) = schema else {
            unreachable!("a JSON object literal")
        };
        Tool::new("echo", "Answer with the text sent", schema)
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
                CallToolResult::success(vec![ContentBlock::text(text)])
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
