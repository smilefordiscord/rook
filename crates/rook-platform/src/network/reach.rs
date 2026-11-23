//! Which hosts a request may reach ([`Reach`]). A game script's request
//! goes where Roblox's game servers can send one: never to Roblox's own
//! web domains, and only to the public internet, unless the host lets
//! scripts reach local addresses (see
//! `docs/behavior/scripting/http-requests.md#where-a-request-may-go`).
//! The checks take a host as the request connects to it, and each
//! address its name resolved to, so a redirect or a name that resolves
//! to a private address can't get round them.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::{NetworkError, Reach};
use crate::endpoints::roblox_host_domain;

/// Why `reach` refuses `host` (a URL's host, before it's resolved), if
/// it does.
pub fn refuse_host(reach: Reach, host: &str) -> Option<NetworkError> {
    match reach {
        Reach::Script { .. } if roblox_host_domain(host).is_some() => {
            Some(NetworkError::RobloxDomain(host.to_owned()))
        }
        _ => None,
    }
}

/// Whether `reach` may connect to `ip`.
pub fn allows(reach: Reach, ip: IpAddr) -> bool {
    match reach {
        Reach::Any | Reach::Script { local: true } => true,
        Reach::Script { local: false } => is_public(ip),
    }
}

/// Whether `ip` is on the public internet: not unspecified, loopback,
/// private (RFC 1918, `fc00::/7`), link-local, shared (CGNAT,
/// `100.64.0.0/10`), multicast, broadcast, or another range IANA keeps
/// off it (documentation, benchmarking, reserved). An IPv4 address carried
/// in IPv6 (mapped, compatible, NAT64, 6to4) is judged as itself.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => match carried_v4(ip) {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(ip),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let this_network = a == 0;
    let shared = a == 100 && (64..128).contains(&b);
    let protocol_assignments = a == 192 && b == 0 && c == 0;
    let benchmarking = a == 198 && (b == 18 || b == 19);
    // 240.0.0.0/4, the broadcast address included.
    let reserved = a >= 240;
    !(this_network
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || shared
        || protocol_assignments
        || ip.is_documentation()
        || benchmarking
        || ip.is_multicast()
        || reserved)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    let unique_local = s[0] & 0xfe00 == 0xfc00;
    let link_local = s[0] & 0xffc0 == 0xfe80;
    // Deprecated, but still not on the internet.
    let site_local = s[0] & 0xffc0 == 0xfec0;
    let discard = s[..4] == [0x100, 0, 0, 0];
    let teredo = s[..2] == [0x2001, 0];
    let documentation = s[..2] == [0x2001, 0xdb8];
    let local_nat64 = s[..3] == [0x64, 0xff9b, 1];
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || unique_local
        || link_local
        || site_local
        || discard
        || teredo
        || documentation
        || local_nat64)
}

/// The IPv4 address an IPv6 one carries, which a connection to it may
/// reach: mapped (`::ffff:a.b.c.d`), translated (`::ffff:0:a.b.c.d`),
/// compatible (`::a.b.c.d`, the unspecified and loopback addresses
/// included), NAT64's well-known prefix (`64:ff9b::/96`) and 6to4
/// (`2002::/16`).
fn carried_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let v4 = |high: u16, low: u16| Some(Ipv4Addr::from((u32::from(high) << 16) | u32::from(low)));
    match ip.segments() {
        [0, 0, 0, 0, 0, 0xffff, high, low]
        | [0, 0, 0, 0, 0xffff, 0, high, low]
        | [0, 0, 0, 0, 0, 0, high, low]
        | [0x64, 0xff9b, 0, 0, 0, 0, high, low]
        | [0x2002, high, low, ..] => v4(high, low),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_internet_addresses_are_public() {
        for ip in [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "127.255.255.254",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
            "100::1",
            "2001::1",
            "2001:db8::1",
            "64:ff9b:1::1",
            "::ffff:127.0.0.1",
            "::ffff:0:10.0.0.1",
            "::192.168.0.1",
            "64:ff9b::a9fe:a9fe",
            "2002:c0a8:0101::1",
        ] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(!is_public(ip), "{ip}");
        }
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "192.0.1.1",
            "198.17.0.1",
            "223.255.255.255",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "2002:0808:0808::1",
        ] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(is_public(ip), "{ip}");
        }
    }

    #[test]
    fn scripts_reach_local_addresses_only_when_allowed() {
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(!allows(Reach::Script { local: false }, loopback));
        assert!(allows(Reach::Script { local: true }, loopback));
        assert!(allows(Reach::Any, loopback));
    }

    /// The host as the client connects to it: any case, with DNS's
    /// trailing dot.
    #[test]
    fn scripts_never_reach_roblox_hosts() {
        for host in ["roblox.com", "WWW.Roblox.com.", "apis.roblox.com"] {
            for reach in [
                Reach::Script { local: false },
                Reach::Script { local: true },
            ] {
                assert_eq!(
                    refuse_host(reach, host),
                    Some(NetworkError::RobloxDomain(host.to_owned()))
                );
            }
            assert_eq!(refuse_host(Reach::Any, host), None);
        }
        assert_eq!(
            refuse_host(Reach::Script { local: false }, "evilroblox.com"),
            None
        );
    }
}
