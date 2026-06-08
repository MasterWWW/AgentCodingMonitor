use std::net::{IpAddr, Ipv4Addr};

/// 判定是否本机 loopback 地址。
pub fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// 判定是否 RFC1918 私网 IPv4（10.x / 172.16–31.x / 192.168.x）。
pub fn is_rfc1918_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private()
}

/// 判定是否链路本地 IPv4（169.254.x.x，APIPA）。
pub fn is_link_local_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_link_local()
}

/// 判定是否可用于局域网配对的 IPv4（RFC1918 或链路本地）。
pub fn is_companion_ipv4(ip: Ipv4Addr) -> bool {
    is_rfc1918_ipv4(ip) || is_link_local_ipv4(ip)
}

/// 按配对优先级排序：RFC1918 私网地址在前，链路本地仅作回退。
pub fn prioritize_companion_ips(ips: Vec<Ipv4Addr>) -> Vec<Ipv4Addr> {
    let mut private: Vec<Ipv4Addr> = ips
        .iter()
        .copied()
        .filter(|ip| is_rfc1918_ipv4(*ip))
        .collect();
    let mut link_local: Vec<Ipv4Addr> = ips
        .iter()
        .copied()
        .filter(|ip| is_link_local_ipv4(*ip))
        .collect();
    private.sort_by_key(|ip| u32::from(*ip));
    link_local.sort_by_key(|ip| u32::from(*ip));
    if !private.is_empty() {
        private
    } else {
        link_local
    }
}

/// 枚举本机可用于局域网配对的 IPv4 地址（RFC1918 优先，链路本地回退）。
pub fn local_ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut raw = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let IpAddr::V4(v4) = iface.ip() {
                if is_companion_ipv4(v4) && !raw.contains(&v4) {
                    raw.push(v4);
                }
            }
        }
    }
    prioritize_companion_ips(raw)
}

/// 获取配对首选 IPv4（列表首项，无私网时回退链路本地）。
pub fn companion_primary_ip() -> Option<Ipv4Addr> {
    local_ipv4_addresses().into_iter().next()
}

/// 首选 IP 是否为链路本地（169.254.x.x），用于托盘警告。
pub fn companion_uses_link_local_fallback() -> bool {
    companion_primary_ip()
        .map(is_link_local_ipv4)
        .unwrap_or(false)
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

/// 构建带 token 的看板 URL 列表（首选 IP 排在首位）。
pub fn companion_urls(port: u16, token: &str) -> Vec<String> {
    local_ipv4_addresses()
        .into_iter()
        .map(|ip| format!("http://{ip}:{port}/mobile?token={token}"))
        .collect()
}

/// 构建首选看板 URL（二维码/剪贴板使用）。
pub fn companion_primary_url(port: u16, token: &str) -> Option<String> {
    companion_primary_ip().map(|ip| format!("http://{ip}:{port}/mobile?token={token}"))
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

    #[test]
    fn prioritize_prefers_rfc1918_over_link_local() {
        let ips = vec![
            Ipv4Addr::new(169, 254, 1, 1),
            Ipv4Addr::new(192, 168, 1, 42),
        ];
        let sorted = prioritize_companion_ips(ips);
        assert_eq!(sorted, vec![Ipv4Addr::new(192, 168, 1, 42)]);
    }

    #[test]
    fn prioritize_falls_back_to_link_local() {
        let ips = vec![Ipv4Addr::new(169, 254, 205, 139)];
        let sorted = prioritize_companion_ips(ips);
        assert_eq!(sorted, vec![Ipv4Addr::new(169, 254, 205, 139)]);
    }

    #[test]
    fn prioritize_sorts_multiple_private_ips() {
        let ips = vec![
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(10, 0, 0, 5),
        ];
        let sorted = prioritize_companion_ips(ips);
        assert_eq!(
            sorted,
            vec![Ipv4Addr::new(10, 0, 0, 5), Ipv4Addr::new(192, 168, 1, 1)]
        );
    }

    #[test]
    fn prioritize_excludes_link_local_when_private_exists() {
        let ips = vec![
            Ipv4Addr::new(169, 254, 205, 139),
            Ipv4Addr::new(10, 0, 0, 5),
            Ipv4Addr::new(192, 168, 1, 42),
        ];
        let sorted = prioritize_companion_ips(ips);
        assert_eq!(
            sorted,
            vec![Ipv4Addr::new(10, 0, 0, 5), Ipv4Addr::new(192, 168, 1, 42)]
        );
    }

    #[test]
    fn companion_primary_url_uses_first_prioritized_ip() {
        let token = "abc123";
        let port = 17392;
        let url = companion_primary_url(port, token);
        // 在无 mock 网卡时依赖本机环境；至少验证格式
        if let Some(u) = url {
            assert!(u.starts_with("http://"));
            assert!(u.contains(&format!(":{port}/mobile?token={token}")));
        }
    }
}
