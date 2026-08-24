# ADR-002: D-Bus Integration (zbus) over Subprocess Calls

## Status
**Accepted** — 2026-08-24

## Context
The Python version shells out to `wpa_cli` with `sudo` for all Wi-Fi Direct operations. This requires root privileges, creates shell injection surface, and produces untyped string output that must be parsed.

## Decision
Use `zbus` (pure-Rust D-Bus client) to communicate directly with:
- `fi.w1.wpa_supplicant1` — P2P device discovery and connection
- `org.freedesktop.NetworkManager` — Higher-level Wi-Fi management
- `org.freedesktop.systemd1` — Service control
- `org.freedesktop.PolicyKit1` — Privilege authorization

## Alternatives Considered
1. **wpactrl crate** (direct Unix socket to wpa_supplicant): Low-quality, unmaintained, no async. D-Bus is the supported stable interface.
2. **Keep subprocess** with capability-based access: Still fragile string parsing, still a security surface.
3. **nmrs crate** (NetworkManager high-level bindings): Good for NM-managed operations. Used as complement to raw zbus where appropriate.

## Consequences
- **Positive:** No sudo, no shell injection, typed responses, async signals, polkit integration automatic
- **Negative:** Need to write `#[proxy]` trait definitions for wpa_supplicant (no pre-made crate)
- **Trade-off:** D-Bus adds ~1ms latency per call (negligible for our use case)
