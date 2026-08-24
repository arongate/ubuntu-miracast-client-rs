# Contributing

## Development Setup

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 2. Install system dependencies (Ubuntu 24.04+)
sudo apt install libgtk-4-dev libadwaita-1-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-plugins-good gstreamer1.0-plugins-ugly \
    pkg-config build-essential

# 3. Clone and build
git clone https://github.com/eddypepy/ubuntu-miracast-client-rs.git
cd ubuntu-miracast-client-rs
cargo build

# 4. Run tests
cargo test

# 5. Run the app
cargo run
```

## Code Quality

All PRs must pass:

```bash
cargo fmt --check          # Formatting
cargo clippy -- -D warnings  # Lints
cargo test                 # Tests
```

## Commit Convention

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add audio streaming support
fix: handle RTSP timeout gracefully
refactor: extract D-Bus proxy into separate module
docs: update architecture diagram
test: add M3 response parsing edge cases
ci: pin cargo-deny version
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for:
- Module map and responsibilities
- Data flow diagrams
- Implementation roadmap
- Protocol conformance status

## Adding a New Module

1. Create `src/mymodule/mod.rs`
2. Add `mod mymodule;` to `src/main.rs`
3. Write tests inline (`#[cfg(test)] mod tests { ... }`)
4. Update ARCHITECTURE.md module table

## Testing

- Unit tests go in the same file as the code (`#[cfg(test)]`)
- Integration tests go in `tests/`
- Use `cargo test module_name` to run specific tests
- Tests requiring GTK/GStreamer init: call `gtk4::init()` or `gstreamer::init()` first
- Tests requiring a display: run with `xvfb-run cargo test` in CI

## D-Bus Integration Pattern

When adding a new D-Bus interface:

```rust
// 1. Define the proxy trait
#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device.WifiP2P",
    default_service = "org.freedesktop.NetworkManager",
)]
trait WifiP2P {
    fn start_find(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;
    fn stop_find(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn peers(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

// 2. Use it
let conn = zbus::Connection::system().await?;
let proxy = WifiP2PProxy::new(&conn, "/org/freedesktop/NetworkManager/Devices/2").await?;
proxy.start_find(HashMap::new()).await?;
```

## GStreamer Pipeline Pattern

```rust
let pipeline = gst::Pipeline::new();
let src = gst::ElementFactory::make("element_name")
    .property("key", value)
    .build()?;
pipeline.add(&src)?;
// ... link elements ...
pipeline.set_state(gst::State::Playing)?;
```

## Release Process

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md
3. `cargo build --release`
4. `cargo deb` for Debian package
5. Tag: `git tag v0.x.y`
6. Push tag triggers CI release workflow
