//! Telegram alerting for oracle errors
//!
//! Alerts interpolate data we do not control: token ids from the request, error text from a
//! custom source's HTTP response, key names, contract ids. Two rules follow from that, and
//! both used to be broken here:
//!
//! 1. **Interpolated text is never markup.** The old code escaped `<`/`>` and then turned
//!    `&lt;b&gt;` back into a real `<b>` — to restore the bold it had just escaped around the
//!    title. That restored the caller's tags too, so any alert text containing `<b>` re-opened
//!    HTML in a message Telegram renders. Bold is now applied only to the operator-authored
//!    title, as literal tags around already-escaped text, and nothing un-escapes anything.
//! 2. **The message never travels in a URL.** It used to be pasted into the query string with
//!    only newlines encoded, so a `#` in a token id or an error truncated the alert at that
//!    point — everything after it became a URL fragment and was silently dropped, which is the
//!    worst possible failure mode for the text explaining what went wrong. It now goes in a
//!    JSON POST body, where no URL syntax can act on it.

use std::env;
use wasi_http_client::Client;

/// Escape the three characters Telegram's HTML parse mode treats as markup.
///
/// That is the complete set for this parse mode (`<`, `>`, `&`) — the entities Telegram
/// supports are all tag-based, so escaping the tag delimiters and the entity introducer leaves
/// nothing that can open markup. `&` MUST be replaced first: doing it later would re-escape
/// the `&` of the `&lt;` this function just produced and print `&amp;lt;` to the operator.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Build the message body: bold title, then the detail on its own lines.
///
/// The `<b>` tags are ours and are the ONLY markup in the result. Both the title and the
/// message are escaped before they are placed inside it, so no interpolated value can close
/// that tag or open another one.
fn format_alert(title: &str, message: &str) -> String {
    format!("<b>{}</b>\n{}", escape_html(title), escape_html(message))
}

/// Serialize the `sendMessage` request body.
///
/// `chat_id` is accepted as a string by the Bot API (numeric ids and `@channelusername` both),
/// and `serde_json` handles the quoting, so an id from the environment cannot break the body
/// the way it could break a query string.
fn alert_body(chat_id: &str, title: &str, message: &str) -> String {
    serde_json::json!({
        "chat_id": chat_id,
        "text": format_alert(title, message),
        "parse_mode": "HTML",
    })
    .to_string()
}

/// Send alert to Telegram (non-blocking, errors are silently ignored)
pub fn send_alert(title: &str, message: &str) {
    let bot_token = match env::var("TELEGRAM_BOT_TOKEN") {
        Ok(t) => t,
        Err(_) => return, // No token configured, skip
    };
    let chat_id = match env::var("TELEGRAM_CHAT_ID") {
        Ok(c) => c,
        Err(_) => return, // No chat ID configured, skip
    };

    // The bot token has to sit in the path — that is the shape of the Bot API — but it comes
    // from the worker's own environment and matches `\d+:[A-Za-z0-9_-]+`. Everything that is
    // not operator-configured travels in the body below.
    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
    let body = alert_body(&chat_id, title, message);

    // Fire and forget - don't block on telegram
    let client = Client::new();
    let _ = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.as_bytes())
        .send();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defect: escaping `<b>` and then un-escaping it again handed markup control to
    /// whoever supplied the text. Token ids, custom-source error bodies and contract ids all
    /// reach these arguments.
    #[test]
    fn interpolated_text_cannot_reopen_html() {
        let alert = format_alert(
            "High Price Deviation",
            "<b>token</b> & <a href=\"http://evil.tld\">click</a>",
        );

        // Exactly one bold pair, and it is the one we wrote around the title
        assert!(alert.starts_with("<b>High Price Deviation</b>\n"));
        assert_eq!(alert.matches("<b>").count(), 1);
        assert_eq!(alert.matches("</b>").count(), 1);

        // Everything from the caller is inert text
        assert!(alert.ends_with(
            "&lt;b&gt;token&lt;/b&gt; &amp; &lt;a href=\"http://evil.tld\"&gt;click&lt;/a&gt;"
        ));
        assert!(!alert.contains("<a "));

        // A caller-supplied title is escaped too, so it cannot break out of our own tags
        let spoofed = format_alert("</b><i>spoof", "detail");
        assert_eq!(spoofed, "<b>&lt;/b&gt;&lt;i&gt;spoof</b>\ndetail");
    }

    /// `&` has to be escaped before `<` and `>`, or the escapes escape each other
    #[test]
    fn ampersand_is_escaped_exactly_once() {
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("&lt;"), "&amp;lt;");
        assert_eq!(escape_html("<>&"), "&lt;&gt;&amp;");
        assert_eq!(escape_html("plain text"), "plain text");
    }

    /// The other half of the defect: the message used to be pasted into the query string with
    /// only `\n` encoded. A `#` cut the alert off at that character — the rest became a URL
    /// fragment the server never saw — and `&` invented new query parameters.
    #[test]
    fn message_survives_url_metacharacters() {
        let message = "Token: usdc#1\nError: HTTP 500 & retry?x=1 100% failed\nkey=value";
        let body = alert_body("-1001234567890", "Custom Data Fetch Error", message);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(
            parsed["text"],
            "<b>Custom Data Fetch Error</b>\nToken: usdc#1\nError: HTTP 500 &amp; retry?x=1 100% failed\nkey=value"
        );
        assert_eq!(parsed["chat_id"], "-1001234567890");
        assert_eq!(parsed["parse_mode"], "HTML");

        // The newline is a real newline in the JSON string, not a `%0A` the reader has to
        // decode, and nothing was dropped at the '#'
        let text = parsed["text"].as_str().unwrap();
        assert_eq!(text.lines().count(), 4);
        assert!(text.contains("key=value"));
        assert!(!text.contains("%0A"));
    }
}
