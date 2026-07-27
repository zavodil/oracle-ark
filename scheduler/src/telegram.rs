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

/// Escape special characters for MarkdownV2.
///
/// The `*bold*` in `send_alert` is operator-authored and wraps text that has already been
/// through here, so nothing interpolated — error strings from the coordinator, WASI failure
/// text, project names — can open markup of its own. The alert also travels as a JSON POST
/// body, so URL syntax never applies to it.
///
/// `\` is in the list because MarkdownV2 treats it as the escape character itself: an
/// unescaped one swallows the character after it (a path or a JSON fragment quoted in an error
/// loses a character), and a trailing one makes Telegram reject the whole request with
/// "can't parse entities" — which drops the alert exactly when something is already wrong. It
/// must stay FIRST in intent: every other entry is escaped by prefixing this character, so it
/// has to be escapable too.
fn escape_markdown(text: &str) -> String {
    let special_chars = [
        '\\', '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.',
        '!',
    ];
    let mut result = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        if special_chars.contains(&c) {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Alerts carry text this process did not author: `anyhow` errors that quote HTTP bodies,
    /// `failures.join("\n")` built from per-group messages, project names from config.
    #[test]
    fn interpolated_text_cannot_open_markdown() {
        // Every MarkdownV2 metacharacter comes back inert
        assert_eq!(escape_markdown("*bold* _it_ `code`"), "\\*bold\\* \\_it\\_ \\`code\\`");
        assert_eq!(escape_markdown("[link](http://evil.tld)"), "\\[link\\]\\(http://evil\\.tld\\)");
        assert_eq!(escape_markdown("100% of 5-10 items!"), "100% of 5\\-10 items\\!");

        // A backslash escapes itself: unescaped, MarkdownV2 would eat the 'n'...
        assert_eq!(escape_markdown("path\\name"), "path\\\\name");
        // ...and a trailing one made Telegram reject the request, losing the alert entirely
        assert_eq!(escape_markdown("truncated\\"), "truncated\\\\");

        // Plain text is untouched
        assert_eq!(escape_markdown("Scheduler failed to fetch prices"), "Scheduler failed to fetch prices");
    }

    /// `#` and `&` are ordinary characters here — the alert is a JSON body, not a query string
    #[test]
    fn message_is_not_url_encoded() {
        let escaped = escape_markdown("Tokens: usdc#1 & wrap.near");
        assert!(escaped.contains('&'));
        assert!(!escaped.contains("%23"));
        // '#' is a MarkdownV2 metacharacter, so it is escaped rather than dropped
        assert_eq!(escaped, "Tokens: usdc\\#1 & wrap\\.near");

        // What actually goes on the wire: reqwest's .json() serializes this verbatim
        let body = serde_json::json!({
            "chat_id": "-1001234567890",
            "text": format!("🚨 *{}*\n\n{}", escape_markdown("WASI Update Failed"), escaped),
            "parse_mode": "MarkdownV2",
        });
        let text = body["text"].as_str().unwrap();
        assert!(text.starts_with("🚨 *WASI Update Failed*\n\n"));
        assert!(text.ends_with("wrap\\.near"));
    }
}
