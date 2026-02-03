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

/// Extract hostname from URL
fn extract_hostname(url: &str) -> Result<String, String> {
    // Simple URL parsing - find hostname between :// and next / or :
    let after_protocol = if let Some(idx) = url.find("://") {
        &url[idx + 3..]
    } else {
        return Err("Invalid URL format".to_string());
    };

    // Find end of hostname (port or path)
    let hostname_end = after_protocol
        .find(|c| c == ':' || c == '/' || c == '?' || c == '#')
        .unwrap_or(after_protocol.len());

    let hostname = &after_protocol[..hostname_end];

    if hostname.is_empty() {
        return Err("Empty hostname".to_string());
    }

    Ok(hostname.to_lowercase())
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
}