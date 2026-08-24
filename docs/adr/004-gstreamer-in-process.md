# ADR-004: In-Process GStreamer Pipeline (gstreamer-rs)

## Status
**Accepted** — 2026-08-24

## Context
The Python version spawns `gst-launch-1.0` as a subprocess. This means:
- No runtime control (can't change bitrate, can't get stats)
- No error handling (parse stderr strings)
- Process lifecycle management (kill, wait, restart)
- No integration with GTK (can't embed video preview)

## Decision
Use `gstreamer-rs` (0.23) to build and manage pipelines in-process.

## Pipeline Architecture
```
ximagesrc → capsfilter(fps) → videoconvert → x264enc → capsfilter(profile) → mpegtsmux → rtpmp2tpay → udpsink
```

### Benefits of In-Process
- **Dynamic control:** Change bitrate, resolution at runtime via element properties
- **Real-time stats:** QoS bus messages give dropped frames, jitter, bitrate
- **Error handling:** Typed GStreamer errors with debug context
- **Hardware acceleration:** Can probe for VA-API elements and swap dynamically
- **GTK integration:** `gtk4paintablesink` for video preview (future)
- **Zero overhead:** No process spawn, no pipe, no stdout parsing

## Future Enhancements
- Auto-detect VA-API: Try `vah264enc` first, fall back to `x264enc`
- PipeWire source: Replace `ximagesrc` with `pipewiresrc` for Wayland
- Audio: Add parallel branch with `pulsesrc → audioconvert → lpcmenc → mux`
- Preview: Tee element → `gtk4paintablesink` for local preview
