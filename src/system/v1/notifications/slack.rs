//! Slack payload rendering.
//!
//! Messages use [Block Kit](https://api.slack.com/block-kit) with a text
//! fallback for notifications. Every interpolated value is escaped because
//! errors include task output, which could otherwise mention users or channels
//! (e.g. `<!channel>`) or render misleading links.

use serde_json::Value;
use serde_json::json;

use super::NotificationMessage;
use super::truncate;

/// The maximum number of characters in a header block.
const MAX_HEADER_CHARS: usize = 150;

/// The maximum number of characters in a section field.
const MAX_FIELD_CHARS: usize = 2000;

/// Renders a message as a Slack incoming webhook payload.
pub(super) fn render(message: &NotificationMessage) -> Value {
    let mut blocks = vec![json!({
        "type": "header",
        "text": {
            "type": "plain_text",
            "text": truncate(&message.title(), MAX_HEADER_CHARS),
        }
    })];

    let fields: Vec<Value> = message
        .fields()
        .into_iter()
        .map(|(name, value)| {
            json!({
                "type": "mrkdwn",
                "text": truncate(&format!("*{name}:*\n{}", escape(&value)), MAX_FIELD_CHARS),
            })
        })
        .collect();

    blocks.push(json!({
        "type": "section",
        "fields": fields,
    }));

    if let Some(error) = &message.error {
        blocks.push(json!({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": format!("*Error:*\n```{}```", escape(error)),
            }
        }));
    }

    if message.suppressed > 0 {
        blocks.push(json!({
            "type": "context",
            "elements": [{
                "type": "mrkdwn",
                "text": message.suppressed_text(),
            }]
        }));
    }

    json!({
        "text": escape(&message.title()),
        "blocks": blocks,
    })
}

/// Escapes the characters that Slack's `mrkdwn` treats as control sequences.
///
/// See <https://api.slack.com/reference/surfaces/formatting#escaping>.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
