# Agent Context — Ubuntu Miracast Client (Rust)

> **For AI agents working on this codebase.** This file provides the context needed to understand the project state, make changes, and continue development.

## Quick Facts

- **Language:** Rust (2024 edition, MSRV 1.85)
- **GUI:** GTK4 (0.9) + libadwaita (0.7) via gtk4-rs
- **Media:** GStreamer (0.23) via gstreamer-rs (in-process pipeline)
- **Networking:** tokio (async TCP/UDP for RTSP), zbus (D-Bus for system services)
- **Config:** serde + toml, XDG paths via `directories` crate
- **Build:** `cargo build`, `cargo test`, `cargo clippy`
- **Tests:** 27 unit tests, all pass, zero warnings

## Project Location

```
/home/epepy/Workspace/linux-remote-screen-cast/ubuntu-miracast-client-rs/
```

Adjacent Python version at:
```
/home/epepy/Workspace/linux-remote-screen-cast/ubuntu-miracast-client/
```

## What's Done

| Module | Status | Tests |
|--------|--------|-------|
| `src/rtsp/messages.rs` | ✅ Complete — RTSP 1.0 parser/builder | 11 tests |
| `src/rtsp/session.rs` | ✅ Complete — async M1-M7 state machine | 2 tests |
| `src/wfd/mod.rs` | ✅ Complete — parameter parsing | 3 tests |
| `src/discovery/mod.rs` | ⚠️ Scaffold — types + WFD parser, D-Bus not wired | 8 tests |
| `src/streaming/mod.rs` | ✅ Complete — GStreamer pipeline builder | 0 tests (needs display) |
| `src/config/mod.rs` | ✅ Complete — XDG + serde + TOML | 3 tests |
| `src/ui/mod.rs` | ⚠️ Scaffold — basic 3-page layout | 0 tests |

## What Needs to Be Done Next (Priority Order)

### 1. Wire D-Bus Discovery (discovery/mod.rs)
Write `#[zbus::proxy]` traits for:
- `fi.w1.wpa_supplicant1.Interface.P2PDevice` — `Find()`, `StopFind()`, `Connect()`
- Signal subscription for `DeviceFound`, `DeviceLost`, `GONegotiationSuccess`
- Parse WFD IEs from discovered peers

### 2. Connect UI to Backend
- Create an application state struct (shared via `Rc<RefCell<AppState>>` or GObject properties)
- Wire "Start Scanning" button to `Discovery::start()`
- Populate device list from D-Bus signals
- Wire device selection to P2P connect → RTSP establish → streaming start

### 3. P2P Connection Flow
After discovery, implement the connection sequence:
1. `P2PDevice.Connect(peer_addr, "pbc", go_intent=0)` via D-Bus
2. Wait for `GroupStarted` signal
3. Get group interface IP via D-Bus or netlink
4. Initiate RTSP session to peer IP

### 4. Audio Support
Add audio branch to GStreamer pipeline:
```
pulsesrc → audioconvert → audio/x-raw,rate=48000,channels=2 → ... → mux
```

### 5. Hardware Acceleration
Probe for VA-API encoder at runtime:
```rust
if gst::ElementFactory::find("vah264enc").is_some() {
    // Use VA-API
} else {
    // Fall back to x264enc
}
```

## Key Patterns in This Codebase

### Error Handling
```rust
// Library-like code (rtsp/): use thiserror for typed errors
#[derive(Debug, Error)]
pub enum ParseError { ... }

// Application code: use anyhow for context
fn do_thing() -> anyhow::Result<()> {
    something().context("failed to do thing")?;
    Ok(())
}
```

### Async + GTK Integration
```rust
// Spawn async work from GTK callback
button.connect_clicked(move |_| {
    let ctx = glib::MainContext::default();
    ctx.spawn_local(async move {
        // async work here (D-Bus calls, RTSP, etc.)
        // Update UI from here (we're on the main thread)
    });
});
```

### GStreamer Pipeline
```rust
let element = gst::ElementFactory::make("element_name")
    .property("key", value)
    .build()?;
pipeline.add(&element)?;
gst::Element::link_many([&src, &element, &sink])?;
pipeline.set_state(gst::State::Playing)?;
```

### D-Bus Proxy (to implement)
```rust
#[zbus::proxy(
    interface = "fi.w1.wpa_supplicant1.Interface.P2PDevice",
    default_service = "fi.w1.wpa_supplicant1",
)]
trait P2PDevice {
    fn find(&self, args: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    fn stop_find(&self) -> zbus::Result<()>;
    fn connect(&self, args: HashMap<&str, Value<'_>>) -> zbus::Result<()>;

    #[zbus(signal)]
    fn device_found(&self, path: OwnedObjectPath) -> zbus::Result<()>;
}
```

## Protocol Reference

The Wi-Fi Display Technical Specification v2.3 (Wi-Fi Alliance, 2024) defines:
- RTSP 1.0 session establishment: M1-M7 messages
- TCP control port: 7236 (default)
- Streaming: RTP/MPEG-TS over UDP (port negotiated in M6 SETUP)
- Mandatory codec: H.264 Constrained Baseline Profile, Level 3.1, 720p30 minimum
- Mandatory audio: LPCM 16-bit 48kHz stereo

See `../ubuntu-miracast-client/docs/ENGINEERING_ANALYSIS.md` for full protocol reference.

## Build Commands

```bash
source "$HOME/.cargo/env"    # If cargo not in PATH
cargo check                  # Fast type-check (no codegen)
cargo build                  # Debug build
cargo build --release        # Release build
cargo test                   # Run all tests
cargo test rtsp              # Run RTSP tests only
cargo clippy -- -D warnings  # Lint
cargo fmt                    # Format
cargo doc --open             # Generate and view docs
```
