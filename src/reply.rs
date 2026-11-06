// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! The tool result every server builds the same way.

use std::fmt;

use rmcp::ErrorData;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;

/// A tool result carrying the same answer twice: as text for the
/// model and as a structured value for the client.
///
/// `Ok(value)` becomes a result whose text is `value`'s `Display` and
/// whose `structuredContent` is `value` serialised; it is flagged
/// `isError` when `is_error(&value)` says so (a validation report that
/// found problems, say). `Err(message)` becomes an `isError` result
/// with the message as its only text. A failure keeps the text only:
/// the structured schema describes a result, and an error is not one.
///
/// A tool that ran and could not do the job answers with a
/// *successful* JSON-RPC response carrying `isError`, so the model
/// sees the text and can react to it. A JSON-RPC error would be
/// handled by the client and never shown.
///
/// # Examples
///
/// ```
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Count {
///     total: usize,
/// }
///
/// impl std::fmt::Display for Count {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{} matches", self.total)
///     }
/// }
///
/// let ok = mcp_serve::reply(Ok(Count { total: 3 }), |_| false).unwrap();
/// assert_eq!(ok.is_error, Some(false));
/// assert_eq!(ok.structured_content.unwrap()["total"], 3);
///
/// let failed =
///     mcp_serve::reply::<Count>(Err("no such file".into()), |_| false)
///         .unwrap();
/// assert_eq!(failed.is_error, Some(true));
/// assert!(failed.structured_content.is_none());
/// ```
///
/// # Errors
///
/// Only when `value` cannot be serialised: an internal error, since
/// the server's own output type is at fault.
pub fn reply<T: Serialize + fmt::Display>(
    outcome: Result<T, String>,
    is_error: impl FnOnce(&T) -> bool,
) -> Result<CallToolResult, ErrorData> {
    match outcome {
        Ok(value) => {
            let structured = serde_json::to_value(&value)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            let content = vec![ContentBlock::text(value.to_string())];
            let mut result = if is_error(&value) {
                CallToolResult::error(content)
            } else {
                CallToolResult::success(content)
            };
            result.structured_content = Some(structured);
            Ok(result)
        }
        Err(message) => {
            Ok(CallToolResult::error(vec![ContentBlock::text(message)]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::{Error as _, Serializer};

    #[derive(Serialize)]
    struct Report {
        problems: Vec<&'static str>,
    }

    impl fmt::Display for Report {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} problems", self.problems.len())
        }
    }

    fn text(result: &CallToolResult) -> &str {
        result.content[0].as_text().map_or("", |t| t.text.as_str())
    }

    #[test]
    fn a_value_is_both_text_and_structure() {
        let result =
            reply(Ok(Report { problems: vec![] }), |r| !r.problems.is_empty())
                .expect("serialises");
        assert_eq!(result.is_error, Some(false));
        assert_eq!(text(&result), "0 problems");
        assert_eq!(
            result.structured_content,
            Some(serde_json::json!({"problems": []}))
        );
    }

    #[test]
    fn a_value_can_be_flagged_as_an_error_and_keeps_its_structure() {
        let report = Report {
            problems: vec!["missing key"],
        };
        let result = reply(Ok(report), |r| !r.problems.is_empty()).expect("ok");
        assert_eq!(result.is_error, Some(true));
        assert_eq!(text(&result), "1 problems");
        assert!(result.structured_content.is_some());
    }

    #[test]
    fn a_failure_is_text_only() {
        let result = reply::<Report>(Err("cannot read x".into()), |_| false)
            .expect("ok");
        assert_eq!(result.is_error, Some(true));
        assert_eq!(text(&result), "cannot read x");
        assert!(result.structured_content.is_none());
    }

    struct Unserialisable;

    impl Serialize for Unserialisable {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(S::Error::custom("refused"))
        }
    }

    impl fmt::Display for Unserialisable {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("unserialisable")
        }
    }

    #[test]
    fn a_value_that_cannot_serialise_is_an_internal_error() {
        let err = reply(Ok(Unserialisable), |_| false).expect_err("error");
        assert!(err.message.contains("refused"), "{err:?}");
    }
}
