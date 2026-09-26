//! A hosted game joined by ticket rather than by address: a QUIC
//! connection to the host's public key, hole-punched where the two
//! networks allow it and carried by a relay where they do not (Phase 4 §6
//! item 3).
//!
//! **Why this, when the router can be asked** (`hosting`). A port mapping
//! needs a router that agrees and a public address to map, and a host
//! behind carrier-grade NAT has neither — no setting on their router can
//! fix that, and until now the answer was Tailscale. A ticket needs
//! nothing of the host's network: both ends dial out, to each other and to
//! a relay, and something always gets through. It also encrypts what the
//! WebSocket sends in the clear (QUIC is TLS 1.3, and the ticket names the
//! key the host must prove).
//!
//! **The WebSocket rides inside the stream.** The joiner opens one QUIC
//! stream and speaks the same WebSocket the TCP server speaks over it,
//! so the server's handshake, lobby and resume run unchanged
//! (`netrunner_server::serve::Acceptor`) and the joiner's driver is the one
//! `remote` driver with a different dial. Rejected: a framing of our own
//! over the raw stream, which would have been a second implementation of
//! the handshake for the server to keep in step with the first — the
//! WebSocket's few bytes of framing a message are not worth that.
//!
//! **The relays are n0's unless the host names one.** iroh's authors run
//! public relays in four regions, free, and they only ever see encrypted
//! QUIC packets between two keys. A relay is the one service a ticket
//! depends on, so the host can name their own instead (`iroh-relay`, one
//! binary; `Settings::relay`), or none — then the ticket carries the
//! host's direct addresses only, which is enough on one network or over
//! IPv6. **The joiner uses whatever relay the ticket names**, so only the
//! host ever chooses. Rejected: n0's DNS address lookup (pkarr), which
//! would let a ticket be the bare key — a second service to depend on, to
//! shorten a string that is pasted rather than typed.
//!
//! **A fresh key per hosted game.** The ticket is good for as long as the
//! host is hosting and no longer. A key that lasts is Phase 4 §5's
//! player identity (`docs/identity-and-rating.md`), which is an Ed25519
//! key as iroh's is; when that key file exists, a host that wants a
//! ticket that survives a restart can be given it.

use std::io;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::Duration;

use iroh::endpoint::{presets, Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, RelayMode, RelayUrl};
use iroh_tickets::endpoint::EndpointTicket;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{oneshot, watch};

/// The protocol a netrunner QUIC connection speaks: a WebSocket, carrying
/// `netrunner_protocol`'s messages as the TCP server's does. A host
/// refuses any other, and the version is in the name so a later framing
/// can be offered beside this one.
pub const ALPN: &[u8] = b"netrunner/ws/1";

/// The path a WebSocket over a ticket asks for. The server never reads
/// the request line; a WebSocket client must still send one.
pub(crate) const WS_URL: &str = "ws://netrunner.peer/";

/// How long a host waits for its relay before giving out a ticket
/// without one. A relay answers in well under a second on a working
/// connection; past this the host is told the ticket may not travel.
const RELAY_DEADLINE: Duration = Duration::from_secs(10);

/// How long a closing stream waits for the other end to have everything
/// it was sent — a refusal, the last `StateUpdate` — before the
/// connection under it is closed.
const LINGER: Duration = Duration::from_secs(5);

/// What a host is joined by: iroh's `EndpointTicket`, the host's key and
/// the addresses and relay it can be reached at, as one string
/// (`endpoint…`, base32).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket(EndpointTicket);

impl Ticket {
    /// Reads what a person pasted, `None` if it is not a ticket — which is
    /// how an address field tells a ticket from an address.
    pub fn parse(input: &str) -> Option<Ticket> {
        EndpointTicket::from_str(input.trim()).ok().map(Ticket)
    }

    /// The relay the host was reached through, if it had one.
    pub fn relay(&self) -> Option<&RelayUrl> {
        self.addr().relay_urls().next()
    }

    fn addr(&self) -> &EndpointAddr {
        self.0.endpoint_addr()
    }

    /// This ticket with its direct addresses struck out: a host a joiner
    /// can reach only through the relay, as behind carrier-grade NAT.
    #[cfg(test)]
    pub(crate) fn relay_only(&self) -> Ticket {
        let addr = EndpointAddr::new(self.addr().id);
        Ticket(EndpointTicket::new(match self.relay() {
            Some(relay) => addr.with_relay_url(relay.clone()),
            None => addr,
        }))
    }
}

impl std::fmt::Display for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Which relay a host is reachable through.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Relay {
    /// n0's public relays; the host is given the nearest.
    #[default]
    Public,
    /// A relay the host runs or trusts (`iroh-relay`).
    Own(RelayUrl),
    /// None: the ticket names the host's direct addresses only.
    Off,
}

impl Relay {
    /// `Settings::relay` read: unset is `Public`, `"off"` is `Off`, and
    /// anything else must be a relay's URL.
    pub fn from_setting(setting: Option<&str>) -> Result<Relay, String> {
        match setting.map(str::trim) {
            None | Some("") => Ok(Relay::Public),
            Some(off) if off.eq_ignore_ascii_case("off") => Ok(Relay::Off),
            Some(url) => url.parse().map(Relay::Own).map_err(|error| format!("{url:?} is not a relay's URL: {error}")),
        }
    }

    fn mode(&self) -> RelayMode {
        match self {
            Relay::Public => RelayMode::Default,
            Relay::Own(url) => RelayMode::custom([url.clone()]),
            Relay::Off => RelayMode::Disabled,
        }
    }
}

/// One QUIC stream, as the byte stream a WebSocket runs over: what a host
/// hands its server and what a joiner's driver dials.
///
/// **Dropping it ends the stream politely.** The send half is finished
/// and the connection under it closed only once the other end has what it
/// was sent, or after `LINGER` — closing a QUIC connection at once throws
/// away whatever is still in flight, and the last thing sent is often the
/// one that matters (a refusal, the end of the match).
pub struct Stream {
    parts: Option<(Connection, SendStream)>,
    recv: RecvStream,
}

impl Stream {
    fn new(connection: Connection, send: SendStream, recv: RecvStream) -> Stream {
        Stream { parts: Some((connection, send)), recv }
    }

    fn send(&mut self) -> &mut SendStream {
        &mut self.parts.as_mut().expect("present until drop").1
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let Some((connection, mut send)) = self.parts.take() else { return };
        let _ = send.finish();
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        runtime.spawn(async move {
            let _ = tokio::time::timeout(LINGER, send.stopped()).await;
            connection.close(0u32.into(), b"done");
        });
    }
}

impl AsyncRead for Stream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.recv).poll_read(cx, buf)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(self.send()).poll_write(cx, buf).map_err(io::Error::other)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(self.send()).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(self.send()).poll_shutdown(cx)
    }
}

async fn bind(relay: &Relay, accept: bool) -> Result<Endpoint, String> {
    let mut builder = Endpoint::builder(presets::Minimal).relay_mode(relay.mode());
    if accept {
        builder = builder.alpns(vec![ALPN.to_vec()]);
    }
    builder.bind().await.map_err(|error| format!("starting the peer-to-peer endpoint: {error}"))
}

/// Where a host's ticket stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    Starting,
    /// Give this out. `relayed` is false when no relay answered in time
    /// (or none was wanted): the ticket then works only where a direct
    /// path does.
    Ready { ticket: Ticket, relayed: bool },
    Failed(String),
}

/// A host's endpoint, accepting joiners by ticket for as long as it lives,
/// polled by a client's frame or tick and never awaited. Each stream a
/// joiner opens is handed to `serve`, which is the server's
/// `Acceptor::serve` in a client that hosts — a closure, because this
/// crate never names the server.
///
/// **Dropping it closes the endpoint**, which ends every stream it
/// accepted: the hosted game goes with its host, as over TCP.
pub struct PeerHost {
    offer: watch::Receiver<Offer>,
    stop: Option<oneshot::Sender<()>>,
}

impl PeerHost {
    /// Starts the endpoint. Must be called inside a tokio runtime.
    pub fn start<F>(relay: Relay, serve: F) -> PeerHost
    where
        F: Fn(Stream, String) + Send + Sync + 'static,
    {
        let (offer_tx, offer) = watch::channel(Offer::Starting);
        let (stop, stopped) = oneshot::channel();
        tokio::spawn(host(relay, serve, offer_tx, stopped));
        PeerHost { offer, stop: Some(stop) }
    }

    pub fn poll(&self) -> Offer {
        self.offer.borrow().clone()
    }

    /// The offer once it is settled, for a caller with nothing to draw.
    pub async fn ready(&mut self) -> Offer {
        let settled = self.offer.wait_for(|offer| *offer != Offer::Starting).await;
        settled.map(|offer| offer.clone()).unwrap_or_else(|_| Offer::Failed("the host stopped".into()))
    }
}

impl Drop for PeerHost {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

async fn host<F>(relay: Relay, serve: F, offer: watch::Sender<Offer>, mut stop: oneshot::Receiver<()>)
where
    F: Fn(Stream, String) + Send + Sync + 'static,
{
    let endpoint = match bind(&relay, true).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            offer.send_replace(Offer::Failed(error));
            return;
        }
    };
    let relayed = relay != Relay::Off && tokio::time::timeout(RELAY_DEADLINE, endpoint.online()).await.is_ok();
    offer.send_replace(Offer::Ready { ticket: Ticket(EndpointTicket::new(endpoint.addr())), relayed });

    let serve = std::sync::Arc::new(serve);
    loop {
        tokio::select! {
            _ = &mut stop => break,
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break };
                let serve = serve.clone();
                tokio::spawn(async move {
                    let Ok(connection) = incoming.await else { return };
                    let who = connection.remote_id().fmt_short().to_string();
                    // One stream per dial today; accepting every one a
                    // connection opens costs nothing and lets a later
                    // client look at the match list and join over one
                    // connection.
                    while let Ok((send, recv)) = connection.accept_bi().await {
                        serve(Stream::new(connection.clone(), send, recv), who.clone());
                    }
                });
            }
        }
    }
    endpoint.close().await;
}

/// A joiner's endpoint, bound on the first dial and kept for the rest —
/// a reconnect dials again from the same key, which the host's relay
/// already knows the way to.
#[derive(Clone, Default)]
pub struct Dialer {
    endpoint: std::sync::Arc<tokio::sync::OnceCell<Endpoint>>,
}

impl Dialer {
    /// A dial that owns what it needs, for a driver that holds it across
    /// a `select!`. The endpoint is bound the first time, through the
    /// relay the ticket names (none if it names none).
    pub fn dial(&self, ticket: &Ticket) -> impl std::future::Future<Output = Result<Stream, String>> + Send + 'static {
        let cell = self.endpoint.clone();
        let ticket = ticket.clone();
        async move {
            let relay = ticket.relay().cloned().map_or(Relay::Off, Relay::Own);
            let endpoint = cell.get_or_try_init(|| bind(&relay, false)).await?;
            let connection = endpoint.connect(ticket.addr().clone(), ALPN).await.map_err(|error| format!("reaching the host: {error}"))?;
            let (send, recv) = connection.open_bi().await.map_err(|error| format!("opening a stream to the host: {error}"))?;
            Ok(Stream::new(connection, send, recv))
        }
    }

    /// Closes the endpoint, if a dial ever bound one.
    pub async fn close(&self) {
        if let Some(endpoint) = self.endpoint.get() {
            endpoint.close().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relay_setting_is_public_unless_it_names_one_or_says_off() {
        assert_eq!(Relay::from_setting(None), Ok(Relay::Public));
        assert_eq!(Relay::from_setting(Some(" ")), Ok(Relay::Public));
        assert_eq!(Relay::from_setting(Some("OFF")), Ok(Relay::Off));
        assert!(matches!(Relay::from_setting(Some("https://relay.example.org")), Ok(Relay::Own(_))));
        assert!(Relay::from_setting(Some("not a url")).is_err());
    }

    /// A ticket survives being printed and pasted back, with the relay it
    /// names; an address is not a ticket.
    #[test]
    fn a_ticket_reads_back_what_it_says() {
        let key = iroh::SecretKey::from_bytes(&[7; 32]).public();
        let relay: RelayUrl = "https://relay.example.org".parse().unwrap();
        let addr = EndpointAddr::new(key).with_relay_url(relay.clone()).with_ip_addr("192.0.2.1:4000".parse().unwrap());
        let ticket = Ticket(EndpointTicket::new(addr));
        let printed = ticket.to_string();
        assert!(printed.starts_with("endpoint"), "{printed}");
        let pasted = Ticket::parse(&format!("  {printed}\n")).expect("surrounding whitespace is not the ticket");
        assert_eq!(pasted, ticket);
        assert_eq!(pasted.relay(), Some(&relay));
        for address in ["ws://127.0.0.1:8080", "192.168.1.5", "endpoint", "[::1]:8080"] {
            assert_eq!(Ticket::parse(address), None, "{address}");
        }
    }
}
