use std::net::{IpAddr, Ipv4Addr};

/// 判定是否本机 loopback 地址。
pub fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// 判定是否常见私网 IPv4（用于生成配对 URL）。
pub fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local()
}

/// 枚举本机可用于局域网配对的 IPv4 地址。
pub fn local_ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut out = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let IpAddr::V4(v4) = iface.ip() {
                if is_private_ipv4(v4) && !out.contains(&v4) {
                    out.push(v4);
                }
            }
        }
    }
    out.sort_by_key(|ip| u32::from(*ip));
    out
}

/// 从查询串或 Authorization 头提取 token。
pub fn extract_token(query: Option<&str>, auth_header: Option<&str>) -> Option<String> {
    if let Some(q) = query {
        for pair in q.split('&') {
            let mut parts = pair.splitn(2, '=');
            if parts.next() == Some("token") {
                if let Some(val) = parts.next() {
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    if let Some(h) = auth_header {
        let prefix = "Bearer ";
        if let Some(rest) = h.strip_prefix(prefix) {
            let t = rest.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// 校验 LAN 请求 token。
pub fn token_matches(provided: Option<&str>, expected: &str) -> bool {
    match provided {
        Some(p) => !expected.is_empty() && p == expected,
        None => false,
    }
}

/// 构建带 token 的看板 URL 列表。
pub fn companion_urls(port: u16, token: &str) -> Vec<String> {
    local_ipv4_addresses()
        .into_iter()
        .map(|ip| format!("http://{ip}:{port}/mobile?token={token}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detects_ipv4() {
        assert!(is_loopback(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(!is_loopback(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn extract_token_from_query() {
        assert_eq!(
            extract_token(Some("token=abc123&x=1"), None).as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn extract_token_from_bearer() {
        assert_eq!(
            extract_token(None, Some("Bearer mytoken")).as_deref(),
            Some("mytoken")
        );
    }

    #[test]
    fn token_matches_expected() {
        assert!(token_matches(Some("abc"), "abc"));
        assert!(!token_matches(Some("wrong"), "abc"));
        assert!(!token_matches(None, "abc"));
    }
}
