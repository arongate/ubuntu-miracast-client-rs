//! GStreamer streaming pipeline for Miracast.
//!
//! Builds and manages an in-process GStreamer pipeline for encoding the screen
//! and streaming via RTP/MPEG-TS to the Miracast sink.

use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::{debug, error, info, warn};

/// Streaming quality presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Low,
    Medium,
    High,
    VeryHigh,
}

impl Quality {
    /// Bitrate in kbps for this quality level.
    pub fn bitrate_kbps(self) -> u32 {
        match self {
            Self::Low => 2000,
            Self::Medium => 5000,
            Self::High => 10000,
            Self::VeryHigh => 20000,
        }
    }
}

/// Configuration for the streaming pipeline.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub target_ip: String,
    pub target_port: u16,
    pub quality: Quality,
    pub framerate: u32,
    pub width: u32,
    pub height: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            target_ip: String::new(),
            target_port: 19000,
            quality: Quality::High,
            framerate: 30,
            width: 1920,
            height: 1080,
        }
    }
}

/// Manages the GStreamer streaming pipeline.
pub struct StreamingPipeline {
    pipeline: Option<gst::Pipeline>,
    config: StreamConfig,
}

impl StreamingPipeline {
    /// Create a new streaming pipeline manager.
    pub fn new(config: StreamConfig) -> Self {
        Self {
            pipeline: None,
            config,
        }
    }

    /// Build and start the streaming pipeline.
    ///
    /// Pipeline: ximagesrc → videoconvert → x264enc → mpegtsmux → rtpmp2tpay → udpsink
    pub fn start(&mut self) -> Result<()> {
        let pipeline = gst::Pipeline::new();

        // Screen capture source (X11)
        let src = gst::ElementFactory::make("ximagesrc")
            .property("use-damage", false)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create ximagesrc: {e}"))?;

        // Video rate control
        let capsfilter = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("framerate", gst::Fraction::new(self.config.framerate as i32, 1))
                    .build(),
            )
            .build()?;

        // Color space conversion
        let videoconvert = gst::ElementFactory::make("videoconvert").build()?;

        // H.264 encoder (Constrained Baseline Profile per WFD spec)
        let encoder = gst::ElementFactory::make("x264enc")
            .property_from_str("tune", "zerolatency")
            .property_from_str("speed-preset", "ultrafast")
            .property("bitrate", self.config.quality.bitrate_kbps())
            .property("key-int-max", self.config.framerate * 2)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create x264enc: {e}. Install gstreamer1.0-plugins-ugly"))?;

        // H.264 caps filter (Constrained Baseline Profile)
        let h264caps = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-h264")
                    .field("profile", "constrained-baseline")
                    .build(),
            )
            .build()?;

        // MPEG-TS muxer
        let muxer = gst::ElementFactory::make("mpegtsmux")
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create mpegtsmux: {e}. Install gstreamer1.0-plugins-bad"))?;

        // RTP packetizer
        let payloader = gst::ElementFactory::make("rtpmp2tpay").build()?;

        // UDP sink
        let sink = gst::ElementFactory::make("udpsink")
            .property("host", &self.config.target_ip)
            .property("port", self.config.target_port as i32)
            .property("sync", false)
            .build()?;

        // Add elements to pipeline
        pipeline.add_many([
            &src,
            &capsfilter,
            &videoconvert,
            &encoder,
            &h264caps,
            &muxer,
            &payloader,
            &sink,
        ])?;

        // Link elements
        gst::Element::link_many([
            &src,
            &capsfilter,
            &videoconvert,
            &encoder,
            &h264caps,
            &muxer,
            &payloader,
            &sink,
        ])?;

        // Set up bus message handling
        let bus = pipeline.bus().unwrap();
        let _watch = bus.add_watch(move |_, msg| {
            use gst::MessageView;
            match msg.view() {
                MessageView::Error(err) => {
                    error!(
                        "GStreamer error: {} (debug: {:?})",
                        err.error(),
                        err.debug()
                    );
                }
                MessageView::Warning(warn) => {
                    warn!("GStreamer warning: {}", warn.error());
                }
                MessageView::Eos(_) => {
                    info!("GStreamer: End of stream");
                }
                MessageView::StateChanged(state) => {
                    if state.src().map(|s| s.is::<gst::Pipeline>()).unwrap_or(false) {
                        debug!(
                            "Pipeline state: {:?} → {:?}",
                            state.old(),
                            state.current()
                        );
                    }
                }
                _ => {}
            }
            glib::ControlFlow::Continue
        })?;

        // Start the pipeline
        pipeline.set_state(gst::State::Playing)?;
        info!(
            "Streaming pipeline started: {}x{}@{}fps {}kbps → {}:{}",
            self.config.width,
            self.config.height,
            self.config.framerate,
            self.config.quality.bitrate_kbps(),
            self.config.target_ip,
            self.config.target_port,
        );

        self.pipeline = Some(pipeline);
        Ok(())
    }

    /// Stop the streaming pipeline.
    pub fn stop(&mut self) -> Result<()> {
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.set_state(gst::State::Null)?;
            info!("Streaming pipeline stopped");
        }
        Ok(())
    }

    /// Whether the pipeline is currently streaming.
    pub fn is_streaming(&self) -> bool {
        self.pipeline
            .as_ref()
            .map(|p| {
                p.current_state() == gst::State::Playing
            })
            .unwrap_or(false)
    }
}

impl Drop for StreamingPipeline {
    fn drop(&mut self) {
        if let Some(pipeline) = self.pipeline.take() {
            let _ = pipeline.set_state(gst::State::Null);
        }
    }
}
