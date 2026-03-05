//! Telegram alert module for scheduler
//!
//! Sends alerts when scheduler encounters errors that WASI can't report
//! (e.g., can't reach coordinator, WASI execution failures)

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Default cooldown between repeated alerts of the same type (10 minutes)
const ALERT_COOLDOWN: Duration = Duration::from_secs(600);

/// Throttles repeated alerts by title. Same alert won't be sent more than once per cooldown.
pub struct AlertThrottle {
    last_sent: HashMap<String, Instant>,
    cooldown: Duration,
}

impl AlertThrottle {
    pub fn new() -> Self {
        Self {
            last_sent: HashMap::new(),
            cooldown: ALERT_COOLDOWN,
        }
    }

    /// Returns true if this alert should be sent (cooldown expired or first time)
    fn should_send(&mut self, title: &str) -> bool {
        match self.last_sent.get(title) {
            Some(last) if last.elapsed() < self.cooldown => false,
            _ => {
                self.last_sent.insert(title.to_string(), Instant::now());
                true
            }
        }
    }

    /// Reset throttle for a given alert (e.g., when condition resolves)
    #[allow(dead_code)]
    pub fn reset(&mut self, title: &str) {
        self.last_sent.remove(title);
    }

    /// Send a throttled alert. Silently skips if same alert was sent within cooldown.
    pub async fn send(
        &mut self,
        client: &reqwest::Client,
        bot_token: Option<&str>,
        chat_id: Option<&str>,
        title: &str,
        message: &str,
    ) {
        if !self.should_send(title) {
            debug!("Telegram alert throttled (cooldown): {}", title);
            return;
        }
        send_alert(client, bot_token, chat_id, title, message).await;
    }
}

/// Send alert to Telegram (unthrottled)
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
