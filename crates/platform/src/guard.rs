use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Loopback,
    LinkLocal,
    Private,
}

pub fn ranges_of(text: &str) -> Vec<Range> {
    let mut ranges: Vec<Range> = text
        .split([',', ' '])
        .filter_map(|name| match name.trim() {
            "loopback" => Some(Range::Loopback),
            "link_local" => Some(Range::LinkLocal),
            "private" => Some(Range::Private),
            _ => None,
        })
        .collect();
    ranges.dedup();
    ranges
}

fn private_v6(ip: std::net::Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn link_local_v6(ip: std::net::Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

fn unwrapped(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

fn within(ip: IpAddr, range: Range) -> bool {
    match (unwrapped(ip), range) {
        (IpAddr::V4(v4), Range::Loopback) => v4.is_loopback() || v4.is_unspecified(),
        (IpAddr::V6(v6), Range::Loopback) => v6.is_loopback() || v6.is_unspecified(),
        (IpAddr::V4(v4), Range::LinkLocal) => v4.is_link_local(),
        (IpAddr::V6(v6), Range::LinkLocal) => link_local_v6(v6),
        (IpAddr::V4(v4), Range::Private) => v4.is_private() || v4.octets()[0] == 100 && v4.octets()[1] & 0xc0 == 64,
        (IpAddr::V6(v6), Range::Private) => private_v6(v6),
    }
}

pub fn blocked(ip: IpAddr, ranges: &[Range]) -> bool {
    ranges.iter().any(|range| within(ip, *range))
}

pub fn literal_blocked(url: &reqwest::Url, ranges: &[Range]) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(v4)) => blocked(IpAddr::V4(v4), ranges),
        Some(url::Host::Ipv6(v6)) => blocked(IpAddr::V6(v6), ranges),
        _ => false,
    }
}

#[derive(Debug, Clone)]
struct Resolver {
    ranges: Arc<Vec<Range>>,
}

impl reqwest::dns::Resolve for Resolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let ranges = Arc::clone(&self.ranges);
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let found: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let allowed: Vec<SocketAddr> = found.into_iter().filter(|addr| !blocked(addr.ip(), &ranges)).collect();
            if allowed.is_empty() {
                let refused = format!("{host} points to an address this server does not call");
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::PermissionDenied, refused))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(allowed.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

pub fn guarded(builder: reqwest::ClientBuilder, ranges: Vec<Range>) -> reqwest::ClientBuilder {
    if ranges.is_empty() {
        return builder;
    }
    let ranges = Arc::new(ranges);
    let kept = Arc::clone(&ranges);
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if literal_blocked(attempt.url(), &kept) {
            attempt.error("the redirect points to an address this server does not call")
        } else {
            attempt.follow()
        }
    });
    builder.dns_resolver(Arc::new(Resolver { ranges })).redirect(policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
    }

    #[test]
    fn the_metadata_address_and_the_loopback_are_blocked_when_asked() {
        let ranges = ranges_of("loopback, link_local");
        assert!(blocked(ip("169.254.169.254"), &ranges));
        assert!(blocked(ip("127.0.0.1"), &ranges));
        assert!(blocked(ip("::ffff:127.0.0.1"), &ranges));
        assert!(blocked(ip("fe80::1"), &ranges));
        assert!(!blocked(ip("10.0.0.5"), &ranges));
        assert!(!blocked(ip("8.8.8.8"), &ranges));
        assert!(blocked(ip("10.0.0.5"), &ranges_of("private")));
        assert!(blocked(ip("100.100.1.1"), &ranges_of("private")));
        assert!(!blocked(ip("127.0.0.1"), &[]));
    }

    #[test]
    fn a_literal_address_in_the_url_is_checked() {
        let ranges = ranges_of("link_local");
        let url = reqwest::Url::parse("http://169.254.169.254/latest").ok();
        assert!(url.is_some_and(|url| literal_blocked(&url, &ranges)));
    }
}
