//! Telegram alerting for oracle errors

use std::env;
use wasi_http_client::Client;

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

    // URL-encode the message
    let text = format!("<b>{}</b>\n{}", title, message)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace("&lt;b&gt;", "<b>")
        .replace("&lt;/b&gt;", "</b>")
        .replace('\n', "%0A");

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage?chat_id={}&parse_mode=HTML&text={}",
        bot_token, chat_id, text
    );

    // Fire and forget - don't block on telegram
    let client = Client::new();
    let _ = client.get(&url).send();
}
