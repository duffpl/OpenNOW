//! LG webOS media runtime for the NVST engine.
//!
//! Mirrors the subset of `opennow-streamer-platform` that `opennow-streamer-core` uses, at the
//! same paths, so the engine builds unchanged. Decode, audio output, and presentation are
//! delegated to a [`MediaBackend`] supplied by the embedding application.

mod backend;
mod cursor;
mod runtime;
mod unsupported;

pub use backend::{MediaBackend, MediaBackendSession};
pub use cursor::{RemoteCursor, RemoteCursorState};
pub use opennow_streamer_media_types::*;
pub use runtime::{MediaControl, MediaRuntime, MediaSession, MediaSink, create_runtime};
pub use unsupported::{
    EncodedRecordingReceiver, MicrophoneReceiver, MicrophoneSession, ReplaySnapshot,
    record_matroska, record_replay_matroska,
};

use opennow_streamer_protocol::VideoBackendCapability;

/// Capabilities are reported by the backend through [`MediaRuntime::video_backends`]; without a
/// runtime there is nothing to decode with.
pub fn video_backends() -> Vec<VideoBackendCapability> {
    Vec::new()
}

/// Every [`MediaBackend`] receives Opus packets through [`MediaBackendSession::push_audio`].
pub const fn supports_audio_decode() -> bool {
    true
}

pub const fn supports_audio_output() -> bool {
    true
}
