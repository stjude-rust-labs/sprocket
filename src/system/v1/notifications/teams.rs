//! Microsoft Teams payload rendering.
//!
//! Messages are [Adaptive Cards](https://adaptivecards.io/), as accepted by
//! Teams Workflows webhooks.

use serde_json::Value;
use serde_json::json;

use super::NotificationMessage;

/// Renders a message as a Teams Workflows webhook payload.
pub(super) fn render(message: &NotificationMessage) -> Value {
    let mut body = vec![json!({
        "type": "TextBlock",
        "text": message.title(),
        "weight": "Bolder",
        "size": "Medium",
        "wrap": true,
    })];

    let facts: Vec<Value> = message
        .fields()
        .into_iter()
        .map(|(title, value)| json!({ "title": title, "value": value }))
        .collect();
    body.push(json!({ "type": "FactSet", "facts": facts }));

    if let Some(error) = &message.error {
        body.push(json!({
            "type": "TextBlock",
            "text": format!("Error: {error}"),
            "wrap": true,
        }));
    }

    if message.suppressed > 0 {
        body.push(json!({
            "type": "TextBlock",
            "text": message.suppressed_text(),
            "isSubtle": true,
            "wrap": true,
        }));
    }

    json!({
        "type": "message",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": {
                "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                "type": "AdaptiveCard",
                "version": "1.4",
                "body": body,
            }
        }]
    })
}
