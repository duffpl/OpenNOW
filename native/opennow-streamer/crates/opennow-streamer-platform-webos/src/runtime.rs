use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, PoisonError, RwLock};

use opennow_streamer_media_types::{
    CapturedInputQueue, EncodedFrame, MediaCodec, MediaFeedback, MediaRuntimeControl,
    MediaStreamConfig, PushOutcome,
};
use opennow_streamer_protocol::{
    AudioDevice, AudioOutputDevice, GraphicsAdapterCapability, RenderSurface, ReplayBufferConfig,
    VideoBackendCapability,
};

use crate::backend::{MediaBackend, MediaBackendSession};
use crate::cursor::RemoteCursor;
use crate::unsupported::{
    EncodedRecordingReceiver, MICROPHONE_UNSUPPORTED, MicrophoneSession, ReplaySnapshot,
};

pub fn create_runtime(backend: Arc<dyn MediaBackend>) -> MediaRuntime {
    MediaRuntime {
        shared: Arc::new(RuntimeShared {
            backend,
            captured_input: Arc::default(),
            cursor: Arc::default(),
            paused: AtomicBool::new(false),
            shut_down: AtomicBool::new(false),
        }),
    }
}

struct RuntimeShared {
    backend: Arc<dyn MediaBackend>,
    captured_input: Arc<CapturedInputQueue>,
    cursor: Arc<RemoteCursor>,
    paused: AtomicBool,
    shut_down: AtomicBool,
}

#[derive(Clone)]
pub struct MediaRuntime {
    shared: Arc<RuntimeShared>,
}

impl MediaRuntime {
    pub fn set_audio_muted(&self, muted: bool) {
        self.shared.backend.set_audio_muted(muted);
    }

    /// The embedding application owns presentation and pointer capture, so local shortcuts that
    /// affect them are forwarded to it as events.
    pub const fn is_embedded(&self) -> bool {
        true
    }

    pub fn video_backends(&self) -> Vec<VideoBackendCapability> {
        self.shared.backend.video_backends()
    }

    pub fn graphics_adapters(&self) -> Vec<GraphicsAdapterCapability> {
        Vec::new()
    }

    /// Queue the embedding application fills with remote input for the active session.
    pub fn captured_input(&self) -> Arc<CapturedInputQueue> {
        Arc::clone(&self.shared.captured_input)
    }

    /// The game's cursor state, for the embedder's pointer mode; reset at every session start.
    pub fn remote_cursor(&self) -> Arc<RemoteCursor> {
        Arc::clone(&self.shared.cursor)
    }

    pub fn start_microphone(&self, _device_id: &str) -> Result<MicrophoneSession, String> {
        Err(MICROPHONE_UNSUPPORTED.to_owned())
    }

    /// The system default output is the only target on webOS.
    pub fn audio_devices(&self) -> Result<Vec<AudioDevice>, String> {
        Ok(Vec::new())
    }

    pub fn start(
        &self,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
    ) -> Result<MediaSession, String> {
        self.start_with_backend(feedback, stream, "auto")
    }

    pub fn validate_backend(&self, requested: &str) -> Result<(), String> {
        if requested == "auto"
            || self
                .video_backends()
                .iter()
                .any(|backend| backend.available && backend.backend == requested)
        {
            Ok(())
        } else {
            Err(format!(
                "Backend {requested} is not supported by this webOS stream view. Select Auto in Stream settings."
            ))
        }
    }

    pub fn start_with_backend(
        &self,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
        requested: &str,
    ) -> Result<MediaSession, String> {
        self.start_with_audio_device(feedback, stream, requested, AudioOutputDevice::default())
    }

    pub fn start_with_audio_device(
        &self,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
        requested: &str,
        _audio_device: AudioOutputDevice,
    ) -> Result<MediaSession, String> {
        if self.shared.shut_down.load(Ordering::Acquire) {
            return Err("native media runtime has shut down".to_owned());
        }
        self.validate_backend(requested)?;
        let backend = self.shared.backend.start(&stream, feedback)?;
        self.shared.cursor.reset();
        let session = MediaSession {
            sink: MediaSink {
                shared: Arc::new(SessionShared {
                    backend: RwLock::new(Some(backend)),
                    paused: AtomicBool::new(false),
                    cursor: Arc::clone(&self.shared.cursor),
                }),
            },
            captured_input: Arc::clone(&self.shared.captured_input),
        };
        if self.shared.paused.load(Ordering::Acquire) {
            session.set_paused(true);
        }
        Ok(session)
    }

    pub fn update_surface(&self, _surface: RenderSurface) -> Result<(), String> {
        Ok(())
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), String> {
        self.shared.paused.store(paused, Ordering::Release);
        Ok(())
    }

    pub fn control(&self, _control: MediaRuntimeControl) -> Result<(), String> {
        Ok(())
    }

    pub fn shutdown(&self) {
        self.shared.shut_down.store(true, Ordering::Release);
    }
}

struct SessionShared {
    // The write lock taken by `stop` guarantees the backend never sees a push after `stop`.
    backend: RwLock<Option<Box<dyn MediaBackendSession>>>,
    paused: AtomicBool,
    cursor: Arc<RemoteCursor>,
}

impl SessionShared {
    fn stop(&self) {
        let backend = self
            .backend
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(backend) = backend {
            backend.stop();
        }
    }
}

#[derive(Clone)]
pub struct MediaSink {
    shared: Arc<SessionShared>,
}

impl MediaSink {
    pub fn push(&self, frame: EncodedFrame) -> PushOutcome {
        let backend = self
            .shared
            .backend
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(backend) = backend.as_ref() else {
            return PushOutcome::Closed;
        };
        if self.shared.paused.load(Ordering::Acquire) {
            return PushOutcome::Paused;
        }
        match frame.codec {
            MediaCodec::H264 | MediaCodec::H265 | MediaCodec::Av1 => backend.push_video(frame),
            MediaCodec::Opus { .. } => backend.push_audio(frame),
            MediaCodec::Unsupported(_) => PushOutcome::Unsupported,
        }
    }
}

pub struct MediaSession {
    sink: MediaSink,
    captured_input: Arc<CapturedInputQueue>,
}

impl MediaSession {
    pub fn captured_input(&self) -> Arc<CapturedInputQueue> {
        Arc::clone(&self.captured_input)
    }

    pub fn sink(&self) -> MediaSink {
        self.sink.clone()
    }

    pub fn control(&self) -> MediaControl {
        MediaControl {
            shared: Arc::clone(&self.sink.shared),
        }
    }

    pub fn set_paused(&self, paused: bool) {
        let shared = &self.sink.shared;
        shared.paused.store(paused, Ordering::Release);
        if let Some(backend) = shared
            .backend
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            backend.set_paused(paused);
        }
    }

    pub fn stop(self) {
        self.sink.shared.stop();
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        self.sink.shared.stop();
    }
}

#[derive(Clone)]
pub struct MediaControl {
    shared: Arc<SessionShared>,
}

impl MediaControl {
    pub fn start_replay(&self, _config: ReplayBufferConfig, _reserved: Arc<AtomicUsize>) {}

    pub fn stop_replay(&self) {}

    pub fn replay_snapshot(&self) -> Result<(MediaStreamConfig, ReplaySnapshot), &'static str> {
        Err("replay-unavailable")
    }

    pub fn subscribe_recording(
        &self,
    ) -> Result<(MediaStreamConfig, EncodedRecordingReceiver), String> {
        Err("recording is unsupported on webOS".to_owned())
    }

    pub fn unsubscribe_recording(&self) {}

    pub fn update_cursor(&self, bytes: Vec<u8>) {
        self.shared.cursor.apply(&bytes);
    }

    pub fn stop(&self) {
        self.shared.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::mpsc::channel;

    use super::*;

    #[derive(Default)]
    struct Recorded {
        video: usize,
        audio: usize,
        paused: Vec<bool>,
        stops: usize,
    }

    #[derive(Default)]
    struct FakeBackend {
        recorded: Arc<Mutex<Recorded>>,
    }

    struct FakeSession {
        recorded: Arc<Mutex<Recorded>>,
    }

    impl MediaBackend for FakeBackend {
        fn video_backends(&self) -> Vec<VideoBackendCapability> {
            vec![VideoBackendCapability {
                backend: "webos",
                platform: "webos",
                codecs: Vec::new(),
                zero_copy_modes: Vec::new(),
                available: true,
                reason: None,
            }]
        }

        fn start(
            &self,
            _config: &MediaStreamConfig,
            _feedback: Sender<MediaFeedback>,
        ) -> Result<Box<dyn MediaBackendSession>, String> {
            Ok(Box::new(FakeSession {
                recorded: Arc::clone(&self.recorded),
            }))
        }
    }

    impl MediaBackendSession for FakeSession {
        fn push_video(&self, _frame: EncodedFrame) -> PushOutcome {
            self.recorded.lock().unwrap().video += 1;
            PushOutcome::Queued
        }

        fn push_audio(&self, _frame: EncodedFrame) -> PushOutcome {
            self.recorded.lock().unwrap().audio += 1;
            PushOutcome::DroppedOldest
        }

        fn set_paused(&self, paused: bool) {
            self.recorded.lock().unwrap().paused.push(paused);
        }

        fn stop(&self) {
            self.recorded.lock().unwrap().stops += 1;
        }
    }

    fn frame(codec: MediaCodec) -> EncodedFrame {
        EncodedFrame {
            mid: "1".to_owned(),
            codec,
            data: Arc::from([0_u8, 0, 0, 1, 0x65]),
            frame_index: Some(3),
            timestamp: 90_000,
            clock_rate_hz: 90_000,
            keyframe: true,
            contiguous: true,
            ssrc: None,
        }
    }

    fn runtime() -> (MediaRuntime, Arc<Mutex<Recorded>>) {
        let backend = FakeBackend::default();
        let recorded = Arc::clone(&backend.recorded);
        (create_runtime(Arc::new(backend)), recorded)
    }

    #[test]
    fn sink_routes_by_codec_and_reports_backend_outcomes() {
        let (runtime, recorded) = runtime();
        let session = runtime
            .start(channel().0, MediaStreamConfig::default())
            .unwrap();
        let sink = session.sink();
        assert_eq!(sink.push(frame(MediaCodec::H265)), PushOutcome::Queued);
        assert_eq!(
            sink.push(frame(MediaCodec::Opus { channels: 2 })),
            PushOutcome::DroppedOldest
        );
        assert_eq!(
            sink.push(frame(MediaCodec::Unsupported("vp9".to_owned()))),
            PushOutcome::Unsupported
        );
        let recorded = recorded.lock().unwrap();
        assert_eq!((recorded.video, recorded.audio), (1, 1));
    }

    #[test]
    fn paused_sessions_reject_frames_without_reaching_the_backend() {
        let (runtime, recorded) = runtime();
        runtime.set_paused(true).unwrap();
        let session = runtime
            .start(channel().0, MediaStreamConfig::default())
            .unwrap();
        assert_eq!(
            session.sink().push(frame(MediaCodec::H264)),
            PushOutcome::Paused
        );
        session.set_paused(false);
        assert_eq!(
            session.sink().push(frame(MediaCodec::H264)),
            PushOutcome::Queued
        );
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded.paused, [true, false]);
        assert_eq!(recorded.video, 1);
    }

    #[test]
    fn backend_is_stopped_once_and_never_sees_later_pushes() {
        let (runtime, recorded) = runtime();
        let session = runtime
            .start(channel().0, MediaStreamConfig::default())
            .unwrap();
        let sink = session.sink();
        session.control().stop();
        assert_eq!(sink.push(frame(MediaCodec::H264)), PushOutcome::Closed);
        session.stop();
        let recorded = recorded.lock().unwrap();
        assert_eq!((recorded.stops, recorded.video), (1, 0));
    }

    #[test]
    fn unknown_backends_and_shut_down_runtimes_refuse_to_start() {
        let (runtime, _) = runtime();
        assert!(runtime.validate_backend("webos").is_ok());
        assert!(
            runtime
                .start_with_backend(channel().0, MediaStreamConfig::default(), "vulkan")
                .is_err()
        );
        runtime.shutdown();
        assert!(
            runtime
                .start(channel().0, MediaStreamConfig::default())
                .is_err()
        );
    }
}
