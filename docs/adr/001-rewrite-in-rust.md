# ADR-001: Rewrite in Rust (from Python)

## Status
**Accepted** — 2026-08-24

## Context
The Python implementation relies on 6 subprocess patterns (wpa_cli, gst-launch-1.0, systemctl, ip) that create security issues (sudo requirement), parsing fragility, and architectural limitations. The GIL limits real-time media handling.

## Decision
Full rewrite in Rust using:
- `gtk4-rs` + `libadwaita-rs` for UI (same look and feel)
- `gstreamer-rs` for in-process media pipeline
- `zbus` for D-Bus integration (wpa_supplicant, NetworkManager, systemd)
- `tokio` for async networking (RTSP session)

## Consequences
- **Positive:** Zero subprocess calls, single binary, type-safe protocol handling, memory safety, 5-10x startup speedup
- **Negative:** 3-4 month learning curve for team, no hot-reload, longer compile times
- **Migration path:** Python version continues working; Rust version developed alongside
