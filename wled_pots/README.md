# wled_pots

Crate for discovering WLED devices on the local network and polling their
inputs (potentiometers on analog pins, touch pads, buttons) over HTTP. The
main app uses it for its **WLED** module and panel.

- Discovery: mDNS browse for `_wled._tcp.local.` (`MdnsDiscovery`), or add a
  device manually by IP (`WledManager::add_manual`) since mDNS on consumer
  networks is known to be unreliable.
- Reading: polls each tracked device's `/json/pins` endpoint and filters for
  pins configured as an "Analog"/"Analog inverted" button (a potentiometer).
- Poll rate: one global interval, adjustable at runtime via
  `WledManager::set_poll_interval`.
- Each request times out after 2 s, so an unplugged device reports an error
  instead of stalling its poller.

## Testing

```
cargo test
```

runs entirely against an in-process mock WLED HTTP server
(`tests/support/mock_wled.rs`) — no real network or hardware required.

Real mDNS discovery can't be exercised in CI/sandboxes (no multicast, no
real device). To check it against actual hardware on your LAN:

```
cargo test -- --ignored
```
