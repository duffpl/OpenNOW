use std::sync::mpsc::Sender;

use opennow_streamer_media_types::{EncodedFrame, MediaFeedback, MediaStreamConfig, PushOutcome};
use opennow_streamer_protocol::VideoBackendCapability;

/// Decode and presentation supplied by the embedding application.
///
/// The engine hands over compressed NVST access units and Opus packets; the backend owns the
/// hardware decoder, audio output, and presentation surface.
pub trait MediaBackend: Send + Sync {
    /// Advertised to the shell during the handshake. The stream is reported as video-capable only
    /// when at least one entry is `available`.
    fn video_backends(&self) -> Vec<VideoBackendCapability>;

    /// Starts decode for one negotiated stream.
    ///
    /// Reject configurations the decoder cannot reproduce exactly (codec, bit depth, chroma,
    /// HDR) instead of degrading them silently. `feedback` stays valid for the whole session and
    /// is the only path for protocol-relevant decoder state:
    ///
    /// - `VideoFrameAccepted` once per access unit the decoder accepted, carrying the frame's
    ///   sender-authored `frame_index`, `timestamp`, byte length and `keyframe` flag. The engine
    ///   derives NVST frame acknowledgements and bitrate telemetry from it.
    /// - `PlaybackStarted` once the first frame is presented.
    /// - `RequestKeyframe` whenever the reference chain is broken (decoder reset, dropped
    ///   reference, discontinuity, resume after pause).
    /// - `QueueDropped` for every frame the backend discards.
    /// - `DecoderError`, `OutputError`, `DeviceLost`, `AudioUnavailable` as they occur.
    /// - `DecodeTimings` is optional; the engine's decode-stall watchdog only runs when it is
    ///   reported.
    fn start(
        &self,
        config: &MediaStreamConfig,
        feedback: Sender<MediaFeedback>,
    ) -> Result<Box<dyn MediaBackendSession>, String>;

    fn set_audio_muted(&self, _muted: bool) {}
}

/// One running decode session.
///
/// Pushes arrive from the engine's media consumer thread while pause and stop arrive from the
/// engine control thread. Pushes must not block on the decoder: the backend bounds its own
/// queues and returns `DroppedOldest` when it evicted a queued frame to accept this one.
pub trait MediaBackendSession: Send + Sync {
    /// H.264, H.265 or AV1 access unit in Annex B / OBU form.
    fn push_video(&self, frame: EncodedFrame) -> PushOutcome;

    /// One Opus packet.
    fn push_audio(&self, frame: EncodedFrame) -> PushOutcome;

    /// While paused the engine stops pushing. Resuming requires a fresh keyframe, which the
    /// backend requests through `RequestKeyframe`.
    fn set_paused(&self, paused: bool);

    /// Releases the decoder and output. Called exactly once; no push follows it.
    fn stop(&self);
}
