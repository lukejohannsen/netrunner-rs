# Playing a friend across the internet

**Play Online → Host a game** runs a game on your machine that you join
yourself. Under **Who can join** pick:

- **this machine only**: for trying it out;
- **anyone on your network**: someone on your wifi joins by the first
  address shown (`ws://192.168.…`);
- **anyone on the internet**: as above, and the waiting screen shows a
  **ticket**: a long line starting `endpoint`. Give your friend that. It
  works from anywhere, whatever your router is like. The client also asks
  your router to forward the port (UPnP, NAT-PMP or PCP), and if the router
  agrees, an address that works from anywhere is listed too.

Your friend chooses **Join a game** and pastes the ticket, or types the
address, into the same field.

## How a ticket gets through

A ticket names your machine by a key, not by an address. Both machines
connect out: to each other where the networks allow it, and to a relay
where they do not. Nothing has to be opened on your router, and it works
behind a provider that shares one address between customers. The game is
encrypted on the way.

The relay is only a go-between for encrypted packets. By default it is one
of the public relays run by n0, the makers of iroh (the library this uses),
and you are given the nearest. To use a relay of your own, run n0's
`iroh-relay` somewhere and put its address in your settings file
(`settings.json` in the client's data folder):

    "relay": "https://relay.example.org"

`"relay": "off"` uses no relay. The ticket then works only where a direct
connection can be made: on your network, or over IPv6. Your friend needs no
setting: their client uses whatever relay your ticket names.

A ticket lasts as long as you are hosting. Host again and you get a new
one.

## When the router says no

You can still give out the ticket. If you would rather give out an
address, the waiting screen says why the router refused and what to do
instead. There are three cases:

- **The router didn't answer.** Automatic port forwarding (usually
  labelled UPnP) is turned off, or your router doesn't support it. You
  can turn it on in the router's settings, or forward the port by hand:
  TCP, the port shown, to the address shown on your network.
- **The router answered but didn't open the port.** Forward the port by
  hand in the router's settings, as above.
- **Your provider shares one address between customers** (carrier-grade
  NAT, common on mobile broadband and some fibre). No setting on your
  router can fix that. Use the ticket, or Tailscale.

## Tailscale: works everywhere, no router settings

[Tailscale](https://tailscale.com) puts your machines on one private
network wherever they are. Both of you install it and join the same
tailnet, or you share your machine with your friend. Then:

1. Host with **anyone on your network**.
2. Give your friend your machine's Tailscale address (`100.x.y.z`, shown
   by `tailscale ip -4`) with the port: `100.x.y.z:8080`.

No port is opened on your router, and the traffic is encrypted.

## IPv6

If your connection has IPv6, the waiting screen also lists a `ws://[…]`
address. A friend who also has IPv6 can use it directly, as long as your
router's firewall lets incoming connections in. Many routers block them
by default.

## Running a server that is always on

`netrunner_server --serve --host :: --port 8080 --bot-runner none` pairs
whoever connects. `::` listens on every IPv6 address, and on IPv4 too on
Linux and macOS. On Windows, use `0.0.0.0` for IPv4. The server doesn't
ask the router for anything: forward the port yourself.
