//! Platform-neutral media types shared by the NVST engine and every media backend.

mod input;
mod shortcuts;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

pub use input::{CapturedInput, CapturedInputQueue, CapturedInputSample};
pub use shortcuts::{ShortcutChord, StreamShortcutAction, StreamShortcutBindings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaVideoCodec {
    H264,
    H265,
    Av1,
}

impl MediaVideoCodec {
    pub const fn label(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "h265",
            Self::Av1 => "av1",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MediaColorQuality {
    #[default]
    EightBit420,
    EightBit444,
    TenBit420,
    TenBit444,
}

impl MediaColorQuality {
    pub const fn protocol_name(self) -> &'static str {
        match self {
            Self::EightBit420 => "8bit_420",
            Self::EightBit444 => "8bit_444",
            Self::TenBit420 => "10bit_420",
            Self::TenBit444 => "10bit_444",
        }
    }

    pub const fn bit_depth(self) -> u8 {
        match self {
            Self::EightBit420 | Self::EightBit444 => 8,
            Self::TenBit420 | Self::TenBit444 => 10,
        }
    }

    pub const fn is_444(self) -> bool {
        matches!(self, Self::EightBit444 | Self::TenBit444)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaStreamConfig {
    pub codec: MediaVideoCodec,
    /// Color class accepted by CloudMatch and requested again during NVST setup.
    pub color_quality: MediaColorQuality,
    pub hdr: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_bps: u32,
    /// CloudMatch accepted Cloud G-SYNC and the host presentation path is VRR-capable.
    pub cloud_gsync: bool,
    pub shortcuts: StreamShortcutBindings,
}

impl Default for MediaStreamConfig {
    fn default() -> Self {
        Self {
            codec: MediaVideoCodec::H264,
            color_quality: MediaColorQuality::default(),
            hdr: false,
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_bps: 75_000_000,
            cloud_gsync: false,
            shortcuts: StreamShortcutBindings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaCodec {
    H264,
    H265,
    Av1,
    Opus { channels: u8 },
    Unsupported(String),
}

#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub mid: String,
    pub codec: MediaCodec,
    pub data: Arc<[u8]>,
    /// Sender-authored NVST video frame index used by the feedback protocol.
    pub frame_index: Option<u32>,
    pub timestamp: u64,
    pub clock_rate_hz: u32,
    pub keyframe: bool,
    pub contiguous: bool,
    pub ssrc: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeStageTimings {
    pub p50_us: u64,
    pub p95_us: u64,
    pub max_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeTimingsReport {
    pub call: Option<DecodeStageTimings>,
    pub residence: Option<DecodeStageTimings>,
    pub call_window_samples: usize,
    pub residence_window_samples: usize,
    pub submissions_total: u64,
    pub outputs_total: u64,
    pub output_calls_total: u64,
    pub last_submission_at: Option<Instant>,
    pub last_output_at: Option<Instant>,
    pub in_flight: usize,
    pub oldest_in_flight_at: Option<Instant>,
    pub epoch: u64,
    pub epoch_started_at: Option<Instant>,
    pub unmatched_outputs: u64,
    pub unmatched_submissions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaFeedback {
    VideoFrameAccepted {
        frame_index: Option<u32>,
        timestamp: u64,
        bytes: u32,
        keyframe: bool,
    },
    PlaybackStarted {
        backend: &'static str,
    },
    BackendFallback {
        from: &'static str,
        to: &'static str,
        reason: String,
    },
    ColorFormatChanged {
        requested: MediaColorQuality,
        actual: MediaColorQuality,
    },
    RequestKeyframe {
        mid: String,
        reason: String,
    },
    DecoderError {
        codec: &'static str,
        message: String,
    },
    AudioDecoderError {
        message: String,
        consecutive: u32,
    },
    AudioUnavailable {
        backend: &'static str,
        reason: String,
        rejected: u64,
    },
    QueueDropped {
        media: &'static str,
        count: usize,
    },
    OutputError {
        message: String,
    },
    DeviceLost {
        subsystem: &'static str,
        recovered: bool,
        message: Option<String>,
    },
    DecodeTimings(DecodeTimingsReport),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaRuntimeControl {
    PointerLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Queued,
    DroppedOldest,
    Paused,
    Unsupported,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingSummary {
    pub path: PathBuf,
    pub video_packets: u64,
    pub audio_packets: u64,
}

pub const MICROPHONE_SAMPLE_RATE: u32 = 48_000;
// The server describes `mic.frameSize:10`: 10 ms at 48 kHz.
pub const MICROPHONE_FRAME_SAMPLES: usize = 480;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedMicrophoneFrame {
    pub opus: Vec<u8>,
    pub timestamp: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrophoneStatus {
    pub enabled: bool,
    pub error: Option<String>,
    pub dropped_pcm_frames: u64,
    pub dropped_encoded_frames: u64,
}
