# Architecture — Ubuntu Miracast Client (Rust)

> **Purpose:** This document captures the system architecture, design decisions, implementation status, and roadmap for AI agents and human contributors to build upon.

## System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    miracast-client (single binary)                │
├─────────────────────────────────────────────────────────────────┤
│  UI Layer (gtk4-rs + libadwaita-rs)                              │
│  ┌──────────────┐ ┌──────────────┐ ┌────────────┐ ┌──────────┐ │
│  │ MainWindow   │ │ DevicePage   │ │ StreamPage │ │ Settings │ │
│  └──────────────┘ └──────────────┘ └────────────┘ └──────────┘ │
├─────────────────────────────────────────────────────────────────┤
│  Application Layer                                               │
│  ┌──────────────┐ ┌──────────────┐ ┌────────────┐ ┌──────────┐ │
│  │ main.rs      │ │ config/      │ │ streaming/ │ │discovery/│ │
│  │ (AdwApp)     │ │ (serde+toml) │ │ (gst-rs)   │ │(zbus)    │ │
│  └──────────────┘ └──────────────┘ └────────────┘ └──────────┘ │
├─────────────────────────────────────────────────────────────────┤
│  Protocol Layer (RTSP + WFD)                                     │
│  ┌──────────────┐ ┌──────────────┐ ┌────────────┐              │
│  │ rtsp/        │ │ rtsp/        │ │ wfd/       │              │
│  │ messages.rs  │ │ session.rs   │ │ mod.rs     │              │
│  │ (parse/build)│ │ (M1-M7 FSM) │ │ (params)   │              │
│  └──────────────┘ └──────────────┘ └────────────┘              │
├─────────────────────────────────────────────────────────────────┤
│  System Integration (all via D-Bus or native APIs)               │
│  ┌──────────────┐ ┌──────────────┐ ┌────────────┐              │
│  │ zbus → NM    │ │ zbus → wpa   │ │ gstreamer  │              │
│  │ (WifiP2P)    │ │ (P2PDevice)  │ │ (in-proc)  │              │
│  └──────────────┘ └──────────────┘ └────────────┘              │
├─────────────────────────────────────────────────────────────────┤
│  OS / Kernel                                                     │
│  ┌──────────────┐ ┌──────────────┐ ┌────────────┐              │
│  │NetworkManager│ │wpa_supplicant│ │ PipeWire/  │              │
│  │  (D-Bus)     │ │  (D-Bus)     │ │ GStreamer  │              │
│  └──────────────┘ └──────────────┘ └────────────┘              │
└─────────────────────────────────────────────────────────────────┘
```

## Module Map

| Module | File(s) | Status | Purpose |
|--------|---------|--------|---------|
| **main** | `src/main.rs` | ✅ Complete | App entry, GStreamer init, AdwApplication launch |
| **rtsp/messages** | `src/rtsp/messages.rs` | ✅ Complete | RTSP 1.0 parser/builder with DoS protection |
| **rtsp/session** | `src/rtsp/session.rs` | ✅ Complete | Async M1-M7 state machine (tokio TCP) |
| **wfd** | `src/wfd/mod.rs` | ✅ Basic | WFD parameter parsing (video/audio/RTP) |
| **discovery** | `src/discovery/mod.rs` | ⚠️ Scaffold | D-Bus types defined, connect/start/stop skeleton |
| **streaming** | `src/streaming/mod.rs` | ✅ Complete | GStreamer pipeline (ximagesrc→x264→mpegts→rtp→udp) |
| **config** | `src/config/mod.rs` | ✅ Complete | XDG paths, serde TOML, defaults |
| **ui** | `src/ui/mod.rs` | ⚠️ Scaffold | Basic 3-page layout (devices, streaming, settings) |

## Key Design Decisions

### 1. Zero Subprocess Architecture
Every external tool call from the Python version is replaced with a native Rust integration:

| Operation | Python (subprocess) | Rust (native) |
|-----------|-------------------|---------------|
| P2P discovery | `wpa_cli p2p_find` | `zbus` → `fi.w1.wpa_supplicant1.Interface.P2PDevice.Find()` |
| P2P connect | `wpa_cli p2p_connect` | `zbus` → D-Bus method call |
| Stream video | `gst-launch-1.0 ...` | `gstreamer-rs` in-process pipeline |
| Manage service | `systemctl --user ...` | `zbus` → `org.freedesktop.systemd1` |
| Get IP/interfaces | `ip addr show` | `rtnetlink` (future) or D-Bus |

### 2. Async-First with Tokio
The RTSP session, D-Bus communication, and network I/O are all async (tokio). The GTK main loop integrates via `glib::MainContext::spawn_local()`.

### 3. Security by Design
- Buffer overflow protection: `MAX_RECV_BUFFER = 65536`, `MAX_CONTENT_LENGTH = 16384`
- Port validation: u16 type enforces range, zero-port defaults to 7236
- No sudo/root: Uses D-Bus (polkit-mediated) instead of subprocess with elevated privileges
- Input validation: Hex string validation before parsing WFD subelements

### 4. Error Handling
- `anyhow::Result` for application errors (propagates context)
- `thiserror` for typed errors in library-like modules (RTSP parsing)
- No panics in production code paths

## Dependency Graph

```
miracast-client
├── gtk4 (0.9) ─── UI framework
├── libadwaita (0.7) ─── GNOME adaptive patterns
├── glib (0.20) ─── GObject runtime
├── gstreamer (0.23) ─── Media pipeline
│   └── gstreamer-video, gstreamer-app
├── tokio (1.x) ─── Async runtime (TCP, timers, sync)
├── zbus (4.x) ─── D-Bus client (NM, wpa_supplicant, systemd)
├── serde (1.x) + toml (0.8) ─── Configuration serialization
├── directories (5.x) ─── XDG Base Directory paths
├── tracing (0.1) ─── Structured logging
├── anyhow (1.x) + thiserror (2.x) ─── Error handling
└── uuid (1.x) ─── Session ID generation
```

## Data Flow: Casting Session

```
1. User clicks "Start Scanning" in UI
   ↓
2. discovery::Discovery::start() → zbus D-Bus call
   → fi.w1.wpa_supplicant1.Interface.P2PDevice.Find()
   ↓
3. D-Bus signals: DeviceFound → populate device list in UI
   ↓
4. User selects device, clicks "Cast"
   ↓
5. P2P connection via D-Bus (GO negotiation, WPS, DHCP)
   ↓
6. rtsp::RtspSession::establish() — async M1-M7 over TCP:7236
   → M1: OPTIONS (query sink)
   → M2: OPTIONS (respond to sink)
   → M3: GET_PARAMETER (get sink capabilities)
   → M4: SET_PARAMETER (set session params)
   → M5: SET_PARAMETER (trigger SETUP)
   → M6: SETUP (sink sends, we respond with session ID)
   → M7: PLAY (sink sends, we respond — streaming begins)
   ↓
7. streaming::StreamingPipeline::start()
   → ximagesrc ! videoconvert ! x264enc ! mpegtsmux ! rtpmp2tpay ! udpsink
   → RTP/MPEG-TS stream sent to negotiated UDP port
   ↓
8. Keep-alive loop (M14: GET_PARAMETER every 15s)
   ↓
9. User clicks "Stop" → rtsp::teardown() + pipeline.stop()
```

## Configuration

Stored at `$XDG_CONFIG_HOME/miracast-client/config.toml` (default: `~/.config/miracast-client/config.toml`):

```toml
[general]
minimize_to_tray = true
start_minimized = false
log_level = "info"

[streaming]
video_quality = "High"
frame_rate = 30
audio_enabled = true

[advanced]
discovery_timeout_secs = 10
connection_timeout_secs = 15
```

## Testing Strategy

| Layer | What to test | How |
|-------|-------------|-----|
| RTSP messages | Parse/serialize correctness, malformed input | Unit tests (11 tests) |
| RTSP session | Full M1-M7 flow, error responses | Mock TCP server (tokio) |
| WFD params | Parameter parsing from real sink responses | Unit tests (3 tests) |
| Discovery | WFD subelement parsing, device types | Unit tests (8 tests) |
| Config | Serialize/deserialize, partial config, defaults | Unit tests (3 tests) |
| Streaming | Pipeline construction, state management | GStreamer test harness |
| UI | Widget creation, signal handling | GTK test utils + xvfb |
| Integration | Full discovery→connect→negotiate→stream | Mock D-Bus + mock sink |

## Implementation Roadmap

### Phase 1: Core Protocol (✅ Done)
- [x] RTSP 1.0 message parser/builder with security bounds
- [x] RTSP/WFD session state machine (async, M1-M7)
- [x] WFD parameter parsing
- [x] GStreamer streaming pipeline (in-process)
- [x] Configuration management (XDG + TOML)
- [x] Application shell (GTK4 + libadwaita)
- [x] Project compiles with zero errors/warnings, 27 tests pass

### Phase 2: D-Bus Integration (Next)
- [ ] Write `#[proxy]` traits for wpa_supplicant P2P interface
- [ ] Implement `Discovery::start()` with real D-Bus calls
- [ ] Subscribe to `DeviceFound`/`DeviceLost` signals
- [ ] Implement P2P connection via D-Bus
- [ ] Handle DHCP (delegate to NetworkManager)
- [ ] Wire discovery results to UI device list

### Phase 3: Full UI (Next)
- [ ] Custom GObject subclass for main window (state management)
- [ ] Device list with AdwActionRow per device
- [ ] Live streaming stats display (bitrate, duration, frames)
- [ ] Settings that persist to config.toml
- [ ] Toast notifications for errors/state changes
- [ ] AdwBreakpoint for responsive layout

### Phase 4: Production Hardening
- [ ] Polkit policy file for privileged operations
- [ ] Linux capabilities (CAP_NET_ADMIN) instead of root
- [ ] AppArmor profile
- [ ] Systemd user service with full hardening
- [ ] Hardware acceleration (VA-API encoder detection)
- [ ] Audio streaming (LPCM via GStreamer)
- [ ] Session history persistence
- [ ] PipeWire screen capture (Wayland support)

### Phase 5: Distribution
- [ ] cargo-deb package with desktop file, icon, service
- [ ] Flatpak manifest (org.gnome.Platform runtime)
- [ ] AppStream metadata
- [ ] Man page
- [ ] Shell completions (clap)

## Protocol Conformance

| WFD Requirement | Status | Notes |
|-----------------|--------|-------|
| Wi-Fi Direct P2P discovery | ⚠️ Scaffold | D-Bus types ready, calls not wired |
| WFD IE advertisement | ❌ TODO | Need to set WFD subelements via D-Bus |
| WFD subelement parsing | ✅ Complete | With full validation |
| P2P connection | ❌ TODO | D-Bus method calls to implement |
| RTSP M1-M7 | ✅ Complete | Async state machine, tested |
| RTSP M8-M9 (teardown) | ✅ Complete | Graceful with timeout |
| RTSP M14 (keep-alive) | ✅ Complete | send_keepalive() method |
| H.264 CBP encoding | ✅ Complete | x264enc with constrained-baseline |
| MPEG-TS muxing + RTP | ✅ Complete | mpegtsmux + rtpmp2tpay |
| LPCM audio | ❌ TODO | Add audio source to pipeline |
| Content protection (HDCP) | ❌ Not planned | Optional per spec |
| UIBC (input back-channel) | ❌ Not planned | Optional per spec |

## Build & Development

```bash
# Prerequisites
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
sudo apt install libgtk-4-dev libadwaita-1-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-plugins-good gstreamer1.0-plugins-ugly pkg-config

# Build
cargo build              # Debug
cargo build --release    # Release (optimized, stripped)

# Test
cargo test               # All tests
cargo test rtsp          # Only RTSP tests

# Lint
cargo clippy -- -D warnings
cargo fmt --check

# Run
cargo run
```

## File Layout

```
ubuntu-miracast-client-rs/
├── Cargo.toml                    # Dependencies, metadata, deb config
├── Cargo.lock                    # Pinned dependency versions
├── README.md                     # User-facing documentation
├── LICENSE                       # MIT
├── ARCHITECTURE.md               # This file
├── SECURITY.md                   # Vulnerability reporting
├── .gitignore
├── src/
│   ├── main.rs                   # Entry point (GStreamer init, AdwApp)
│   ├── rtsp/
│   │   ├── mod.rs                # Module re-exports
│   │   ├── messages.rs           # RTSP 1.0 parser/builder (551 LOC)
│   │   └── session.rs           # Async session state machine (449 LOC)
│   ├── wfd/
│   │   └── mod.rs                # WFD parameter types + parsing
│   ├── discovery/
│   │   └── mod.rs                # D-Bus P2P discovery + WFD subelement parser
│   ├── streaming/
│   │   └── mod.rs                # GStreamer pipeline management
│   ├── config/
│   │   └── mod.rs                # XDG + serde + TOML config
│   └── ui/
│       └── mod.rs                # GTK4/libadwaita views
├── data/
│   ├── com.github.eddypepy.MiracastClient.desktop
│   └── icons/
│       └── com.github.eddypepy.MiracastClient.svg
├── .github/
│   └── workflows/
│       └── ci.yml                # Build + test + clippy + fmt
└── tests/                        # Integration tests (future)
```

## Comparison with Python Version

| Metric | Python | Rust |
|--------|--------|------|
| Source lines | ~4,500 | ~1,957 |
| Test count | 208 | 27 (growing) |
| Subprocess calls | 6 patterns | 0 |
| Startup time | 400-1000ms | 100-200ms |
| Binary size | N/A (interpreter) | ~5 MB (stripped) |
| Memory safety | Runtime (exceptions) | Compile-time (ownership) |
| Concurrency | GIL + threads | Async (tokio) + Send/Sync |
| Protocol correctness | Runtime errors | Type-enforced states |
