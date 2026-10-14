//! Capture and export paths the webOS runtime does not provide. The handle types are
//! uninhabited: the runtime never produces them, so the engine's code paths that consume them
//! type-check without carrying an encoder or muxer.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::RecvError;

use opennow_streamer_media_types::{
    EncodedFrame, EncodedMicrophoneFrame, MediaStreamConfig, MicrophoneStatus, RecordingSummary,
};

pub(crate) const MICROPHONE_UNSUPPORTED: &str = "microphone capture is unsupported on webOS";

pub enum MicrophoneSession {}

impl MicrophoneSession {
    pub fn receiver(&self) -> MicrophoneReceiver {
        match *self {}
    }

    pub fn try_recv(&mut self) -> Result<Option<EncodedMicrophoneFrame>, String> {
        match *self {}
    }

    pub fn status(&mut self) -> MicrophoneStatus {
        match *self {}
    }

    pub fn stop(&mut self) {
        match *self {}
    }
}

/// Inhabited, unlike [`MicrophoneSession`], so the engine's uplink setup after
/// `MicrophoneSession::receiver` is not flagged as unreachable. Never constructed.
pub struct MicrophoneReceiver {
    _private: (),
}

impl MicrophoneReceiver {
    pub fn try_recv(&self) -> Result<Option<EncodedMicrophoneFrame>, String> {
        Err(MICROPHONE_UNSUPPORTED.to_owned())
    }

    pub fn status(&self) -> MicrophoneStatus {
        MicrophoneStatus {
            enabled: false,
            error: Some(MICROPHONE_UNSUPPORTED.to_owned()),
            dropped_pcm_frames: 0,
            dropped_encoded_frames: 0,
        }
    }

    pub fn stop(&self) {}
}

pub enum ReplaySnapshot {}

pub enum EncodedRecordingReceiver {}

impl EncodedRecordingReceiver {
    pub fn recv(&self) -> Result<EncodedFrame, RecvError> {
        match *self {}
    }

    pub fn overflowed(&self) -> bool {
        match *self {}
    }
}

pub fn record_matroska(
    _output_path: impl AsRef<Path>,
    _stream: MediaStreamConfig,
    receiver: EncodedRecordingReceiver,
) -> Result<RecordingSummary, String> {
    match receiver {}
}

pub fn record_replay_matroska(
    _output_path: impl AsRef<Path>,
    _stream: MediaStreamConfig,
    snapshot: ReplaySnapshot,
    _cancelled: &AtomicBool,
) -> Result<RecordingSummary, String> {
    match snapshot {}
}
