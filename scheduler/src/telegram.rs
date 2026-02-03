//! Telegram alert module for scheduler
//!
//! Sends alerts when scheduler encounters errors that WASI can't report
//! (e.g., can't reach coordinator, WASI execution failures)

use tracing::{debug, warn};

/// Send alert to Telegram
/// Returns silently if TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID are not set
pub async fn send_alert(client: &reqwest::Client, bot_token: Option<&str>, chat_id: Option<&str>, title: &str, message: &str) {
    let (Some(token), Some(chat)) = (bot_token, chat_id) else {
        debug!("Telegram not configured, skipping alert: {}", title);
        return;
    };

    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
    let text = format!("🚨 *{}*\n\n{}", escape_markdown(title), escape_markdown(message));

    let result = client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": chat,
            "text": text,
            "parse_mode": "MarkdownV2"
        }))
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            debug!("Telegram alert sent: {}", title);
        }
        Ok(resp) => {
            warn!("Telegram API error: {} - {:?}", resp.status(), resp.text().await);
        }
        Err(e) => {
            warn!("Failed to send Telegram alert: {}", e);
        }
    }
}

/// Escape special characters for MarkdownV2
fn escape_markdown(text: &str) -> String {
    let special_chars = ['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!'];
    let mut result = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        if special_chars.contains(&c) {
            result.push('\\');
        }
        result.push(c);
    }
    result
}
