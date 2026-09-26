//! A game hosted on a player's own machine, reachable by the people they
//! give an address to: the listening socket, the addresses to hand out,
//! and the router asked to open the port (Phase 4 §6).
//!
//! **Why here and not in `netrunner_server`.** The server serves a
//! listener (`Server::from_listener`) and takes no socket or router
//! dependency of its own (AGENTS.md: no I/O dependencies added to the
//! server); what a *host* needs — a dual-stack socket, the machine's own
//! addresses, a port mapping — is a client's concern, and toolkit-agnostic,
//! so the terminal's Host a game and the desktop's online screen share it.
//!
//! **The router is asked, not trusted.** UPnP-IGD, NAT-PMP and PCP are how
//! a program on a home network asks the router to forward a port, and
//! `portmapper` speaks all three. Many routers have them off, and a router
//! behind carrier-grade NAT gets a mapping on an address that is itself
//! shared with other customers — so a mapping is only reported as
//! reachable when its external address is a public one
//! ([`classify_external`]), and every failure says what the host can do
//! instead: forward the port by hand, or put both machines on Tailscale.
//!
//! **What this does not do** is get through a NAT without the router's
//! help. That needs a rendezvous and a relay and a UDP transport, and is
//! `peer` (Phase 4 §6, item 3): STUN alone only tells a machine its
//! public address.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, TcpListener};
use std::num::NonZeroU16;
use std::time::{Duration, Instant};

/// The port a host offers by default, and the one a joined address gets
/// when it names none — `netrunner_server --serve`'s default.
pub const DEFAULT_PORT: u16 = 8080;

/// Who a hosted game is for, which decides what the socket listens on and
/// whether the router is asked to open the port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Loopback only: for trying it out, or two clients on one machine.
    ThisMachine,
    /// Every address this machine has, IPv4 and IPv6.
    Network,
    /// As `Network`, and the router is asked to forward the port — and
    /// the host is given a ticket (`peer`), which needs no router at all.
    Internet,
}

impl Reach {
    pub const ALL: [Reach; 3] = [Reach::ThisMachine, Reach::Network, Reach::Internet];

    pub fn label(self) -> &'static str {
        match self {
            Reach::ThisMachine => "this machine only (for trying it out)",
            Reach::Network => "anyone on your network",
            Reach::Internet => "anyone on the internet (a ticket, and asks your router to open the port)",
        }
    }

    /// The next choice, wrapping; `back` goes the other way.
    pub fn step(self, back: bool) -> Reach {
        let at = Self::ALL.iter().position(|reach| *reach == self).expect("every reach is listed");
        let len = Self::ALL.len();
        Self::ALL[if back { (at + len - 1) % len } else { (at + 1) % len }]
    }

    fn asks_the_router(self) -> bool {
        self == Reach::Internet
    }
}

/// Binds the host's listening socket. `port` 0 takes any free port.
///
/// **One dual-stack socket, not two listeners.** `[::]` with
/// `IPV6_V6ONLY` off takes IPv4 connections as mapped addresses, and the
/// option is set explicitly because the platforms disagree on its default
/// (on for Windows, off for Linux unless a sysctl says otherwise). Two
/// listeners were the alternative, and `Server` accepts from one. Where
/// IPv6 is unavailable — disabled, or no `::` to bind — this falls back to
/// `0.0.0.0`, which is what a host got before.
pub fn bind_listener(reach: Reach, port: u16) -> std::io::Result<TcpListener> {
    if reach == Reach::ThisMachine {
        return TcpListener::bind((Ipv4Addr::LOCALHOST, port));
    }
    bind_dual_stack(port).or_else(|_| TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)))
}

fn bind_dual_stack(port: u16) -> std::io::Result<TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_only_v6(false)?;
    // What `std`'s own `TcpListener::bind` sets on Unix, so a host that
    // quits and hosts again is not refused the port for TIME_WAIT's
    // minutes; Windows gives the option a different, unsafe meaning.
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)).into())?;
    socket.listen(128)?;
    Ok(socket.into())
}

/// The URL a client dials for `addr`. `SocketAddr`'s own formatting
/// brackets an IPv6 address, which a URL requires.
pub fn ws_url(addr: SocketAddr) -> String {
    format!("ws://{addr}")
}

/// What a person types, as the URL the client dials: `ws://` when no
/// scheme is given, and the default port when none is. An IPv6 address is
/// bracketed if it was typed bare (`::1`), because a URL cannot say where
/// the address ends and the port begins otherwise. A ticket is left alone.
pub fn normalize_address(input: &str) -> String {
    let input = input.trim();
    // A host's ticket is dialled as it is (`peer`, `remote`).
    if crate::peer::Ticket::parse(input).is_some() {
        return input.to_string();
    }
    let (scheme, rest) = match input.split_once("://") {
        Some((scheme, rest)) => (scheme, rest),
        None => ("ws", input),
    };
    let rest = rest.trim_end_matches('/');
    if let Ok(ip) = rest.parse::<Ipv6Addr>() {
        return format!("{scheme}://[{ip}]:{DEFAULT_PORT}");
    }
    // After a bracketed address only `]:port` can name a port; anywhere
    // else the last colon does.
    let after_host = match rest.rfind(']') {
        Some(end) => &rest[end + 1..],
        None => rest,
    };
    let has_port = after_host.rsplit_once(':').is_some_and(|(_, port)| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()));
    if has_port { format!("{scheme}://{rest}") } else { format!("{scheme}://{rest}:{DEFAULT_PORT}") }
}

/// This machine's address on its IPv4 network, as an opponent there would
/// dial it: the source address the OS would use to reach a
/// documentation-range outside address. Connecting a UDP socket sends
/// nothing — it only asks the routing table. `None` with no route.
pub fn lan_ipv4() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

/// This machine's global IPv6 address, found the same way against the
/// IPv6 documentation prefix (`2001:db8::/32`). Only a global unicast
/// address is kept: a link-local or unique-local one cannot be dialled
/// from outside, and the LAN IPv4 already covers the network.
pub fn global_ipv6() -> Option<Ipv6Addr> {
    let socket = std::net::UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V6(ip) if is_global_unicast(ip) => Some(ip),
        _ => None,
    }
}

/// `2000::/3`, less the documentation prefix. `Ipv6Addr::is_global` is
/// still unstable, and the one range an address worth sharing sits in is
/// simple to say.
fn is_global_unicast(ip: Ipv6Addr) -> bool {
    let [first, second, ..] = ip.segments();
    (first & 0xe000) == 0x2000 && !(first == 0x2001 && second == 0x0db8)
}

/// One address a host can give out, and who can use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub url: String,
    pub who: &'static str,
}

/// The addresses to give an opponent for a game listening on `port`,
/// nearest first. The router's external address is not among them: it
/// arrives later, from a [`PortMapper`].
pub fn share_addresses(reach: Reach, port: u16) -> Vec<Share> {
    share_addresses_from(reach, port, lan_ipv4(), global_ipv6())
}

fn share_addresses_from(reach: Reach, port: u16, lan: Option<Ipv4Addr>, v6: Option<Ipv6Addr>) -> Vec<Share> {
    if reach == Reach::ThisMachine {
        return vec![Share { url: ws_url((Ipv4Addr::LOCALHOST, port).into()), who: "this machine" }];
    }
    let mut shares = vec![match lan {
        Some(ip) => Share { url: ws_url((ip, port).into()), who: "your network" },
        None => Share { url: format!("ws://<this machine's address>:{port}"), who: "your network" },
    }];
    if let Some(ip) = v6 {
        shares.push(Share { url: ws_url((ip, port).into()), who: "anywhere over IPv6, if your router's firewall lets it in" });
    }
    shares
}

/// Where a request to the router stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mapping {
    Asking,
    /// The router forwards this public address to the host.
    Open(SocketAddrV4),
    Failed(MappingFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingFailure {
    /// No UPnP, NAT-PMP or PCP answered: they are off on the router, or it
    /// has none.
    NoAnswer,
    /// Something answered, and no mapping came back in time.
    Refused,
    /// A mapping came back on an address that is not public — carrier-grade
    /// NAT (`100.64.0.0/10`) or a private range, so the router is itself
    /// behind another NAT, the ISP's, and no mapping on it reaches the
    /// internet.
    SharedAddress(Ipv4Addr),
}

impl MappingFailure {
    /// What went wrong and what to do instead, for the host's status.
    pub fn advice(&self, port: u16, lan: Option<Ipv4Addr>) -> String {
        let why = match self {
            MappingFailure::NoAnswer => "Your router did not answer a request to open the port (UPnP, NAT-PMP and PCP are off, or it has none).".to_string(),
            MappingFailure::Refused => "Your router answered but did not open the port.".to_string(),
            MappingFailure::SharedAddress(ip) => format!(
                "Your router's outside address is {ip}, which your internet provider shares with other customers, so opening a port on the router does not reach the internet."
            ),
        };
        let forward = match (self, lan) {
            (MappingFailure::SharedAddress(_), _) => String::new(),
            (_, Some(ip)) => format!(" Forward TCP port {port} to {ip} in your router's settings, or"),
            (_, None) => format!(" Forward TCP port {port} to this machine in your router's settings, or"),
        };
        let tailscale = if forward.is_empty() { " Instead," } else { "" };
        format!("{why}{forward}{tailscale} both install Tailscale and give your opponent this machine's Tailscale address.")
    }
}

/// Judges a mapping's external address: public is reachable, anything else
/// is the router sitting behind someone else's NAT.
pub fn classify_external(addr: SocketAddrV4) -> Mapping {
    let ip = *addr.ip();
    let [a, b, ..] = ip.octets();
    let carrier_grade = a == 100 && (64..128).contains(&b);
    if carrier_grade || ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
        Mapping::Failed(MappingFailure::SharedAddress(ip))
    } else {
        Mapping::Open(addr)
    }
}

/// A request to the router for a port, polled by a client's frame or tick
/// and never awaited — discovery takes seconds. A trait so a screen's
/// handling of every answer is tested without a router.
pub trait PortMapper: Send {
    fn poll(&mut self) -> Mapping;
}

/// How long a router that answered the probe has to produce a mapping.
/// UPnP discovery and a mapping round trip are a few seconds on a
/// healthy network; past this the host is better off told.
const MAPPING_DEADLINE: Duration = Duration::from_secs(20);

/// The real thing, over `portmapper`: UPnP-IGD, NAT-PMP and PCP for a TCP
/// port. The mapping is renewed while this lives. **Dropping it releases
/// the mapping** — `portmapper`'s service would abort without releasing if
/// simply dropped, so the drop deactivates it and keeps the service alive
/// a moment for the release to go out. A process that dies first leaves a
/// UPnP mapping to its two-hour lease, and NAT-PMP and PCP ones to theirs.
pub struct RouterMapping {
    client: portmapper::Client,
    external: tokio::sync::watch::Receiver<Option<SocketAddrV4>>,
    probe: Option<tokio::sync::oneshot::Receiver<Result<portmapper::ProbeOutput, portmapper::ProbeError>>>,
    answered: bool,
    started: Instant,
    settled: Option<Mapping>,
}

impl RouterMapping {
    /// Starts asking for `port`. Must be called inside a tokio runtime.
    pub fn start(port: u16) -> Self {
        let client = portmapper::Client::new(portmapper::Config {
            enable_upnp: true,
            enable_pcp: true,
            enable_nat_pmp: true,
            protocol: portmapper::Protocol::Tcp,
        });
        let probe = client.probe();
        if let Some(port) = NonZeroU16::new(port) {
            client.update_local_port(port);
        }
        let external = client.watch_external_address();
        RouterMapping { client, external, probe: Some(probe), answered: false, started: Instant::now(), settled: None }
    }
}

impl PortMapper for RouterMapping {
    fn poll(&mut self) -> Mapping {
        if let Some(external) = *self.external.borrow() {
            // Renewal can move the address; the latest one is the answer.
            let judged = classify_external(external);
            self.settled = Some(judged.clone());
            return judged;
        }
        if let Some(settled) = &self.settled {
            return settled.clone();
        }
        if let Some(probe) = &mut self.probe {
            match probe.try_recv() {
                Ok(Ok(output)) => {
                    self.probe = None;
                    self.answered = output.upnp || output.pcp || output.nat_pmp;
                    if !self.answered {
                        self.settled = Some(Mapping::Failed(MappingFailure::NoAnswer));
                        return Mapping::Failed(MappingFailure::NoAnswer);
                    }
                }
                Ok(Err(_)) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    // No gateway, or the service is gone: nobody to ask.
                    self.probe = None;
                    self.settled = Some(Mapping::Failed(MappingFailure::NoAnswer));
                    return Mapping::Failed(MappingFailure::NoAnswer);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            }
        }
        if self.started.elapsed() > MAPPING_DEADLINE {
            let failure = if self.answered { MappingFailure::Refused } else { MappingFailure::NoAnswer };
            self.settled = Some(Mapping::Failed(failure.clone()));
            return Mapping::Failed(failure);
        }
        Mapping::Asking
    }
}

impl Drop for RouterMapping {
    fn drop(&mut self) {
        self.client.deactivate();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let client = self.client.clone();
            runtime.spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                drop(client);
            });
        }
    }
}

/// Starts a router request when `reach` wants one.
pub fn map_port(reach: Reach, port: u16) -> Option<Box<dyn PortMapper>> {
    reach.asks_the_router().then(|| Box::new(RouterMapping::start(port)) as Box<dyn PortMapper>)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_get_a_scheme_and_a_port() {
        assert_eq!(normalize_address("192.168.1.5"), "ws://192.168.1.5:8080");
        assert_eq!(normalize_address("192.168.1.5:9000"), "ws://192.168.1.5:9000");
        assert_eq!(normalize_address(" ws://host.example:8080/ "), "ws://host.example:8080");
        assert_eq!(normalize_address("wss://host.example"), "wss://host.example:8080");
    }

    #[test]
    fn an_ipv6_address_is_bracketed_and_its_port_found_after_the_bracket() {
        assert_eq!(normalize_address("::1"), "ws://[::1]:8080");
        assert_eq!(normalize_address("2001:db8::5"), "ws://[2001:db8::5]:8080", "a bare address's last group is not a port");
        assert_eq!(normalize_address("[::1]"), "ws://[::1]:8080");
        assert_eq!(normalize_address("[::1]:9000"), "ws://[::1]:9000");
        assert_eq!(normalize_address("ws://[2001:db8::5]:9000/"), "ws://[2001:db8::5]:9000");
    }

    #[test]
    fn a_url_brackets_ipv6() {
        assert_eq!(ws_url((Ipv6Addr::LOCALHOST, 8080).into()), "ws://[::1]:8080");
        assert_eq!(ws_url((Ipv4Addr::new(192, 168, 1, 5), 8080).into()), "ws://192.168.1.5:8080");
    }

    #[test]
    fn reach_steps_both_ways_and_wraps() {
        assert_eq!(Reach::Network.step(false), Reach::Internet);
        assert_eq!(Reach::Network.step(true), Reach::ThisMachine);
        assert_eq!(Reach::Internet.step(false), Reach::ThisMachine);
        assert_eq!(Reach::ThisMachine.step(true), Reach::Internet);
    }

    #[test]
    fn only_a_public_external_address_is_open() {
        let at = |a, b, c, d| SocketAddrV4::new(Ipv4Addr::new(a, b, c, d), 8080);
        assert_eq!(classify_external(at(203, 0, 113, 7)), Mapping::Open(at(203, 0, 113, 7)));
        for shared in [at(100, 64, 0, 1), at(100, 127, 255, 254), at(10, 0, 0, 1), at(192, 168, 0, 2), at(172, 16, 5, 5)] {
            assert_eq!(classify_external(shared), Mapping::Failed(MappingFailure::SharedAddress(*shared.ip())), "{shared}");
        }
        assert!(matches!(classify_external(at(100, 128, 0, 1)), Mapping::Open(_)), "just past carrier-grade NAT is public");
    }

    #[test]
    fn a_failure_says_what_to_do_instead() {
        let lan = Some(Ipv4Addr::new(192, 168, 1, 20));
        let no_answer = MappingFailure::NoAnswer.advice(8080, lan);
        assert!(no_answer.contains("Forward TCP port 8080 to 192.168.1.20") && no_answer.contains("Tailscale"), "{no_answer}");
        let shared = MappingFailure::SharedAddress(Ipv4Addr::new(100, 70, 1, 1)).advice(8080, lan);
        assert!(!shared.contains("Forward"), "forwarding cannot help behind the provider's NAT: {shared}");
        assert!(shared.contains("Tailscale"), "{shared}");
    }

    #[test]
    fn a_host_shares_the_network_address_then_a_global_ipv6_one() {
        let lan = Some(Ipv4Addr::new(192, 168, 1, 20));
        let v6 = Some("2a02:1234::5".parse().unwrap());
        let shares = share_addresses_from(Reach::Network, 8080, lan, v6);
        assert_eq!(shares.iter().map(|share| share.url.as_str()).collect::<Vec<_>>(), ["ws://192.168.1.20:8080", "ws://[2a02:1234::5]:8080"]);
        assert_eq!(share_addresses_from(Reach::ThisMachine, 8080, lan, v6)[0].url, "ws://127.0.0.1:8080");
        assert!(share_addresses_from(Reach::Network, 8080, None, None)[0].url.contains("<this machine's address>"));
    }

    #[test]
    fn only_global_unicast_ipv6_is_worth_sharing() {
        assert!(is_global_unicast("2a02:1234::5".parse().unwrap()));
        for local in ["fe80::1", "fd00::1", "::1", "2001:db8::1"] {
            assert!(!is_global_unicast(local.parse().unwrap()), "{local}");
        }
    }

    /// One socket takes both families. Skipped where the box has no IPv6
    /// loopback, which the fallback exists for.
    #[test]
    fn the_network_socket_accepts_ipv4_and_ipv6() {
        let listener = bind_listener(Reach::Network, 0).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("IPv4 reaches the socket");
        if listener.local_addr().unwrap().is_ipv6() {
            std::net::TcpStream::connect((Ipv6Addr::LOCALHOST, port)).expect("IPv6 reaches the socket");
        }
    }

    #[test]
    fn this_machine_only_is_loopback() {
        let listener = bind_listener(Reach::ThisMachine, 0).unwrap();
        assert_eq!(listener.local_addr().unwrap().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
}
