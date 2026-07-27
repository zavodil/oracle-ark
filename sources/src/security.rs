//! Guards for the one URL a caller gets to choose: the custom source.
//!
//! This module lives in the SHARED crate, not in the `oracle-ark` binary where it started,
//! because both crates fetch caller-supplied URLs: `oracle-ark`'s `fetch_custom_raw` and
//! `oracle_ark_sources::sources::sync::fetch_custom`. The second one had no validation at all
//! while the first did, which is what a second copy of a security check always degrades into —
//! one copy gets fixed and the other keeps the hole. There is exactly one implementation here,
//! it is not feature-gated, and its tests run with the crate's default features.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Validates that a URL does not point to local or private network resources
pub fn validate_url(url: &str) -> Result<(), String> {
    // Block file:// and unix:// protocols
    if url.starts_with("file://") || url.starts_with("unix://") {
        return Err("Access to local file system is blocked".to_string());
    }

    // Extract hostname from URL
    let hostname = extract_hostname(url)?;

    // Block localhost variations
    if hostname == "localhost" ||
       hostname == "localhost.localdomain" ||
       hostname.ends_with(".local") ||
       hostname.ends_with(".localhost") {
        return Err("Access to localhost is blocked".to_string());
    }

    // Try to parse as IP address
    if let Ok(ip) = hostname.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(ipv4) => {
                // Block loopback (127.0.0.0/8)
                if ipv4.is_loopback() {
                    return Err("Access to loopback address is blocked".to_string());
                }

                // Block private networks (RFC 1918)
                if ipv4.is_private() {
                    return Err("Access to private network is blocked".to_string());
                }

                // Block link-local (169.254.0.0/16)
                if ipv4.is_link_local() {
                    return Err("Access to link-local address is blocked".to_string());
                }

                // Block multicast and broadcast
                if ipv4.is_multicast() || ipv4.is_broadcast() {
                    return Err("Access to multicast/broadcast address is blocked".to_string());
                }

                // Block 0.0.0.0
                if ipv4 == Ipv4Addr::new(0, 0, 0, 0) {
                    return Err("Access to 0.0.0.0 is blocked".to_string());
                }
            }
            IpAddr::V6(ipv6) => {
                // Block loopback (::1)
                if ipv6.is_loopback() {
                    return Err("Access to IPv6 loopback is blocked".to_string());
                }

                // Block link-local and unique local addresses
                if is_ipv6_private(&ipv6) {
                    return Err("Access to private IPv6 network is blocked".to_string());
                }

                // Block multicast
                if ipv6.is_multicast() {
                    return Err("Access to IPv6 multicast is blocked".to_string());
                }
            }
        }
    }

    // Block Docker internal DNS
    if hostname == "host.docker.internal" || hostname == "gateway.docker.internal" {
        return Err("Access to Docker internal network is blocked".to_string());
    }

    // Block Kubernetes DNS
    if hostname.ends_with(".cluster.local") ||
       hostname.ends_with(".svc") ||
       hostname == "kubernetes" ||
       hostname == "kubernetes.default" {
        return Err("Access to Kubernetes internal network is blocked".to_string());
    }

    Ok(())
}

/// Extract the host a URL actually connects to.
///
/// Every guard in this module is a decision about that host, so the parsing must be the
/// same one the HTTP client performs — a host that merely LOOKS trusted is what turns a
/// check into a bypass.
fn extract_hostname(url: &str) -> Result<String, String> {
    let after_protocol = if let Some(idx) = url.find("://") {
        &url[idx + 3..]
    } else {
        return Err("Invalid URL format".to_string());
    };

    // Cut the authority off at the first path/query/fragment separator before anything else,
    // so a '@' or ':' further down the path cannot be read as userinfo or a port.
    let authority_end = after_protocol
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(after_protocol.len());
    let authority = &after_protocol[..authority_end];

    // Everything up to the LAST '@' is userinfo, not the host:
    // `http://api.coingecko.com@127.0.0.1/` connects to 127.0.0.1. Keeping the credential
    // in the string makes it neither a recognisable IP nor an exact allowlist entry, which
    // is precisely how a crafted URL slips past a host check.
    let authority = match authority.rfind('@') {
        Some(idx) => &authority[idx + 1..],
        None => authority,
    };

    // IPv6 literals are bracketed (`http://[::1]:8080/x`). They must be unwrapped first: the
    // generic scan below would stop at the first ':' inside the address and yield "[", which
    // then fails to parse as an IpAddr and slips past the loopback/private guards entirely.
    let hostname = if let Some(rest) = authority.strip_prefix('[') {
        match rest.find(']') {
            Some(end) => &rest[..end],
            None => return Err("Malformed IPv6 URL: missing ']'".to_string()),
        }
    } else {
        // Find end of hostname (port)
        let hostname_end = authority.find(':').unwrap_or(authority.len());
        &authority[..hostname_end]
    };

    // A resolver treats `localhost.` and `localhost` as the same name, so the trailing root
    // label has to go before any comparison — otherwise one character defeats every check.
    let hostname = hostname.trim_end_matches('.');

    if hostname.is_empty() {
        return Err("Empty hostname".to_string());
    }

    Ok(hostname.to_lowercase())
}

/// Hosts allowed to receive the `API_KEY` secret.
///
/// The custom-source URL is chosen by the caller while `API_KEY` is a credential the enclave
/// holds on our behalf (CoinGecko Pro, Alchemy). Sending it with every request meant one call
/// to `https://attacker.tld/` handed that credential over, so it now travels only to the
/// providers that issued it. Adding an entry here gives its operator our key — a deliberate
/// decision, not a configuration detail.
const API_KEY_HOSTS: [&str; 2] = ["pro-api.coingecko.com", "g.alchemy.com"];

/// Whether `url` may carry the `API_KEY` secret.
///
/// Matches the host exactly or on a dot boundary, so `evil-coingecko.com` (no boundary) and
/// `pro-api.coingecko.com.attacker.tld` (right suffix, wrong place) are both refused. HTTPS
/// is required because plaintext would put the key on the wire for anyone on the path, and a
/// URL this module cannot parse simply fails to match: the header is dropped, never guessed.
pub fn may_receive_api_key(url: &str) -> bool {
    match url.find("://") {
        Some(idx) if url[..idx].eq_ignore_ascii_case("https") => {}
        _ => return false,
    }

    let hostname = match extract_hostname(url) {
        Ok(hostname) => hostname,
        Err(_) => return false,
    };

    API_KEY_HOSTS.iter().any(|allowed| {
        hostname
            .strip_suffix(allowed)
            .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with('.'))
    })
}

/// Check if IPv6 address is private (link-local or unique local)
fn is_ipv6_private(ipv6: &Ipv6Addr) -> bool {
    let segments = ipv6.segments();

    // Link-local (fe80::/10)
    if segments[0] & 0xffc0 == 0xfe80 {
        return true;
    }

    // Unique local (fc00::/7)
    if segments[0] & 0xfe00 == 0xfc00 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_localhost() {
        assert!(validate_url("http://localhost/api").is_err());
        assert!(validate_url("http://localhost:8080/api").is_err());
        assert!(validate_url("https://localhost/").is_err());
        assert!(validate_url("http://LOCALHOST/").is_err());
    }

    #[test]
    fn test_block_loopback() {
        assert!(validate_url("http://127.0.0.1/").is_err());
        assert!(validate_url("http://127.0.0.1:3000/").is_err());
        assert!(validate_url("http://127.255.255.255/").is_err());
    }

    #[test]
    fn test_block_private_networks() {
        assert!(validate_url("http://10.0.0.1/").is_err());
        assert!(validate_url("http://172.16.0.1/").is_err());
        assert!(validate_url("http://192.168.1.1/").is_err());
        assert!(validate_url("http://169.254.1.1/").is_err());
    }

    #[test]
    fn test_block_special_addresses() {
        assert!(validate_url("http://0.0.0.0/").is_err());
        assert!(validate_url("http://255.255.255.255/").is_err());
        assert!(validate_url("http://224.0.0.1/").is_err()); // multicast
    }

    #[test]
    fn test_block_file_protocols() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("unix:///var/run/dstack.sock").is_err());
    }

    #[test]
    fn test_block_docker_kubernetes() {
        assert!(validate_url("http://host.docker.internal/").is_err());
        assert!(validate_url("http://gateway.docker.internal/").is_err());
        assert!(validate_url("http://kubernetes/").is_err());
        assert!(validate_url("http://myservice.default.svc.cluster.local/").is_err());
    }

    #[test]
    fn test_allow_public_urls() {
        assert!(validate_url("https://api.coingecko.com/api/v3/simple/price").is_ok());
        assert!(validate_url("https://api.binance.com/api/v3/ticker/price").is_ok());
        assert!(validate_url("https://8.8.8.8/dns").is_ok());
        assert!(validate_url("https://example.com/api").is_ok());
    }

    #[test]
    fn test_ipv6() {
        assert!(validate_url("http://[::1]/").is_err()); // loopback
        assert!(validate_url("http://[fe80::1]/").is_err()); // link-local
        assert!(validate_url("http://[fc00::1]/").is_err()); // unique local
        assert!(validate_url("http://[2001:db8::1]/").is_ok()); // public IPv6
    }

    /// The host we check has to be the host the client dials, or the guards are decorative
    #[test]
    fn test_hostname_is_the_host_actually_dialed() {
        // Userinfo: this connects to 127.0.0.1, whatever it claims before the '@'
        assert_eq!(
            extract_hostname("http://api.coingecko.com@127.0.0.1/x").unwrap(),
            "127.0.0.1"
        );
        assert!(validate_url("http://api.coingecko.com@127.0.0.1/x").is_err());
        assert!(validate_url("http://user:pass@169.254.169.254/latest/meta-data/").is_err());

        // A trailing root label resolves identically, so it must not read as a new name
        assert_eq!(extract_hostname("http://localhost./").unwrap(), "localhost");
        assert!(validate_url("http://localhost./").is_err());

        // '@' and ':' below the authority belong to the path, not to the host
        assert_eq!(
            extract_hostname("https://example.com/redirect?to=a@evil.tld").unwrap(),
            "example.com"
        );
        assert_eq!(
            extract_hostname("https://example.com:8443/a:b").unwrap(),
            "example.com"
        );
    }

    /// `API_KEY` is an OutLayer-managed credential and the URL is caller-supplied, so this
    /// list is the only thing standing between the two
    #[test]
    fn test_api_key_only_goes_to_allowlisted_hosts() {
        assert!(may_receive_api_key("https://pro-api.coingecko.com/api/v3/simple/price"));
        assert!(may_receive_api_key("https://eth-mainnet.g.alchemy.com/v2"));
        assert!(may_receive_api_key("https://PRO-API.CoinGecko.com/api/v3/ping"));
        assert!(may_receive_api_key("https://pro-api.coingecko.com:443/api/v3/ping"));

        // The attack that motivated the allowlist: any URL at all used to get the header
        assert!(!may_receive_api_key("https://attacker.tld/"));

        // Lookalikes must fail on the dot boundary, in both directions
        assert!(!may_receive_api_key("https://evil-coingecko.com/"));
        assert!(!may_receive_api_key("https://pro-api.coingecko.com.attacker.tld/"));
        assert!(!may_receive_api_key("https://coingecko.com/"));
        assert!(!may_receive_api_key("https://g.alchemy.com.evil.tld/v2"));

        // Nor may a crafted authority smuggle a trusted-looking name past the check
        assert!(!may_receive_api_key("https://pro-api.coingecko.com@attacker.tld/"));
        assert!(!may_receive_api_key("https://attacker.tld/pro-api.coingecko.com"));
        assert!(!may_receive_api_key("https://attacker.tld/?x=pro-api.coingecko.com"));

        // Plaintext would leak the key to anyone on the path, allowlisted host or not
        assert!(!may_receive_api_key("http://pro-api.coingecko.com/api/v3/ping"));

        // Unparseable input drops the header instead of guessing
        assert!(!may_receive_api_key("pro-api.coingecko.com"));
        assert!(!may_receive_api_key(""));
    }

    /// The URLs `sources::sync::fetch_custom` must refuse before it opens a connection.
    ///
    /// That function is `pub` and had no validation at all while this module sat one crate
    /// away, so it would happily fetch the cloud metadata endpoint and return the body — the
    /// credentials it serves included. It now calls `validate_url` itself; this pins the
    /// specific targets that matters for.
    #[test]
    fn test_custom_source_urls_that_must_be_refused() {
        // Cloud instance metadata — the classic SSRF payoff, and it answers over plain HTTP
        assert!(validate_url("http://169.254.169.254/latest/meta-data/iam/security-credentials/").is_err());
        assert!(validate_url("http://[fd00:ec2::254]/latest/meta-data/").is_err());

        // Services on the worker host or its private network
        assert!(validate_url("http://127.0.0.1:8080/admin").is_err());
        assert!(validate_url("http://10.0.0.5:6379/").is_err());
        assert!(validate_url("http://localhost:8081/decrypt").is_err());

        // The dstack guest agent socket the TEE stack exposes
        assert!(validate_url("unix:///var/run/dstack.sock").is_err());
        assert!(validate_url("file:///proc/self/environ").is_err());

        // A real custom source still works — the guard blocks destinations, not the feature
        assert!(validate_url("https://api.exchange.example/v1/ticker?symbol=NEARUSD").is_ok());
    }
}