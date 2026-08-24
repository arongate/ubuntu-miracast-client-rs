# ADR-003: Tokio Async Runtime for Network Operations

## Status
**Accepted** — 2026-08-24

## Context
The RTSP session negotiation, D-Bus communication, and keep-alive loops all involve network I/O with timeouts. Need to run concurrently without blocking the GTK main loop.

## Decision
Use `tokio` as the async runtime for all network operations. Integrate with GTK's GLib main loop via `glib::MainContext::spawn_local()` for UI updates.

## Architecture Pattern
```
┌─────────────────────┐      ┌──────────────────────┐
│  GLib Main Loop     │      │  Tokio Runtime       │
│  (GTK UI events)    │◄────►│  (RTSP, D-Bus, UDP)  │
│  spawn_local()      │      │  spawn()             │
└─────────────────────┘      └──────────────────────┘
```

- UI callbacks → spawn tokio tasks for async work
- Tokio tasks → send results back via `glib::MainContext::channel()`
- GStreamer runs on its own thread pool (managed by GStreamer internally)

## Alternatives Considered
1. **async-std**: Smaller ecosystem, fewer D-Bus integrations
2. **GLib async (gio::Task)**: Would work but limited compared to tokio ecosystem
3. **Blocking threads**: Simpler but wastes resources, harder timeout handling

## Consequences
- **Positive:** Efficient concurrent I/O, natural timeout handling, huge ecosystem
- **Negative:** Two event loops (GLib + tokio) adds complexity
- **Mitigation:** Well-established pattern used by COSMIC desktop apps
