//! Audio sink abstractions and hardware implementations.
//!
//! Provides the `AudioSink` trait, an AAudio (Android NDK low-latency mode) implementation
//! for Meta Quest 3 on Horizon OS (`target_os = "android"`), comprehensive architectural notes
//! on OpenSL ES fallback, and a `MockAudioSink` for unit tests and desktop Linux environments.

use std::fmt::Debug;
use thiserror::Error;

/// Error variants from audio sink operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AudioSinkError {
    /// Failed to open audio device or stream.
    #[error("Failed to open audio sink: {0}")]
    Open(String),

    /// Failed to start playback.
    #[error("Failed to start audio playback: {0}")]
    Start(String),

    /// Failed to pause playback.
    #[error("Failed to pause audio playback: {0}")]
    Pause(String),

    /// Failed to stop playback or flush buffers.
    #[error("Failed to stop audio playback: {0}")]
    Stop(String),

    /// Error writing audio samples to output hardware.
    #[error("Failed to write audio samples: {0}")]
    Write(String),

    /// Hardware audio sink is not opened or ready.
    #[error("Audio sink is not ready or has been disconnected")]
    NotReady,

    /// Audio buffer underrun occurred.
    #[error("Audio buffer underrun occurred")]
    Underrun,
}

/// Configuration options for audio sinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSinkConfig {
    /// Sample rate in Hz (e.g. 48,000 Hz).
    pub sample_rate: u32,
    /// Channel count (e.g. 2 for stereo).
    pub channels: u16,
    /// Target buffer capacity in audio frames (default: 480 frames = 10ms @ 48kHz).
    pub buffer_capacity_frames: u32,
    /// Request low-latency performance mode from the audio HAL.
    pub low_latency: bool,
}

impl Default for AudioSinkConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            buffer_capacity_frames: 480, // 10ms
            low_latency: true,
        }
    }
}

/// Common abstraction for hardware and simulated audio output sinks.
///
/// Intentionally not `Send + Sync`: `AAudioSink` wraps raw NDK stream handles that are not
/// thread-safe in Rust's type system, and every method is called through `&mut self`/`&self`
/// by a single owner.
pub trait AudioSink {
    /// Initializes and opens the audio sink with the given configuration.
    fn open(&mut self, config: AudioSinkConfig) -> Result<(), AudioSinkError>;

    /// Starts audio playback stream.
    fn start(&mut self) -> Result<(), AudioSinkError>;

    /// Pauses audio playback without dropping queued frames.
    fn pause(&mut self) -> Result<(), AudioSinkError>;

    /// Stops audio playback and flushes hardware buffers.
    fn stop(&mut self) -> Result<(), AudioSinkError>;

    /// Writes interleaved 32-bit floating point audio samples `[-1.0, 1.0]` to the output sink.
    ///
    /// Returns the number of samples actually written.
    fn write_samples(&mut self, samples: &[f32]) -> Result<usize, AudioSinkError>;

    /// Current hardware output latency estimate in frames.
    fn latency_frames(&self) -> Result<u32, AudioSinkError>;

    /// Frames written but not yet played, if the sink can tell. Playout uses it to keep a
    /// steady cushion against network jitter.
    fn queued_frames(&self) -> Option<u32> {
        None
    }

    /// Whether the audio sink is currently active and playing.
    fn is_playing(&self) -> bool;

    /// Current audio sink configuration.
    fn config(&self) -> &AudioSinkConfig;
}

impl<S: ?Sized + AudioSink> AudioSink for Box<S> {
    fn open(&mut self, config: AudioSinkConfig) -> Result<(), AudioSinkError> {
        (**self).open(config)
    }

    fn start(&mut self) -> Result<(), AudioSinkError> {
        (**self).start()
    }

    fn pause(&mut self) -> Result<(), AudioSinkError> {
        (**self).pause()
    }

    fn stop(&mut self) -> Result<(), AudioSinkError> {
        (**self).stop()
    }

    fn write_samples(&mut self, samples: &[f32]) -> Result<usize, AudioSinkError> {
        (**self).write_samples(samples)
    }

    fn latency_frames(&self) -> Result<u32, AudioSinkError> {
        (**self).latency_frames()
    }

    fn queued_frames(&self) -> Option<u32> {
        (**self).queued_frames()
    }

    fn is_playing(&self) -> bool {
        (**self).is_playing()
    }

    fn config(&self) -> &AudioSinkConfig {
        (**self).config()
    }
}

/// Software mock audio sink for unit testing, CI pipelines, and desktop host environments.
pub struct MockAudioSink {
    config: AudioSinkConfig,
    is_open: bool,
    is_playing: bool,
    written_samples: Vec<f32>,
    total_samples_written: usize,
    simulated_latency_frames: u32,
    simulated_queued_frames: Option<u32>,
    pub should_fail: bool,
}

impl MockAudioSink {
    /// Creates a new mock audio sink with the specified configuration.
    pub fn new(config: AudioSinkConfig) -> Self {
        Self {
            config,
            is_open: false,
            is_playing: false,
            written_samples: Vec::new(),
            total_samples_written: 0,
            simulated_latency_frames: 120, // 2.5ms simulated latency @ 48kHz
            simulated_queued_frames: None,
            should_fail: false,
        }
    }

    /// Access to all raw audio samples written to this sink since creation or last clear.
    pub fn written_samples(&self) -> &[f32] {
        &self.written_samples
    }

    /// Total count of samples written over the sink's lifetime.
    pub fn total_samples_written(&self) -> usize {
        self.total_samples_written
    }

    /// Clears recorded sample buffer.
    pub fn clear(&mut self) {
        self.written_samples.clear();
    }

    /// Overrides simulated output latency in frames.
    pub fn set_simulated_latency_frames(&mut self, frames: u32) {
        self.simulated_latency_frames = frames;
    }

    /// Sets what [`AudioSink::queued_frames`] reports (`None`, the default, reports nothing).
    pub fn set_simulated_queued_frames(&mut self, frames: Option<u32>) {
        self.simulated_queued_frames = frames;
    }
}

impl AudioSink for MockAudioSink {
    fn open(&mut self, config: AudioSinkConfig) -> Result<(), AudioSinkError> {
        if self.should_fail {
            return Err(AudioSinkError::Open("Simulated open failure".into()));
        }
        self.config = config;
        self.is_open = true;
        self.written_samples.clear();
        self.total_samples_written = 0;
        Ok(())
    }

    fn start(&mut self) -> Result<(), AudioSinkError> {
        if !self.is_open {
            return Err(AudioSinkError::NotReady);
        }
        if self.should_fail {
            return Err(AudioSinkError::Start("Simulated start failure".into()));
        }
        self.is_playing = true;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), AudioSinkError> {
        if !self.is_open {
            return Err(AudioSinkError::NotReady);
        }
        self.is_playing = false;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AudioSinkError> {
        self.is_playing = false;
        self.is_open = false;
        Ok(())
    }

    fn write_samples(&mut self, samples: &[f32]) -> Result<usize, AudioSinkError> {
        if !self.is_open || !self.is_playing {
            return Err(AudioSinkError::NotReady);
        }
        if self.should_fail {
            return Err(AudioSinkError::Write("Simulated write failure".into()));
        }
        self.written_samples.extend_from_slice(samples);
        self.total_samples_written += samples.len();
        Ok(samples.len())
    }

    fn latency_frames(&self) -> Result<u32, AudioSinkError> {
        Ok(self.simulated_latency_frames)
    }

    fn queued_frames(&self) -> Option<u32> {
        self.simulated_queued_frames
    }

    fn is_playing(&self) -> bool {
        self.is_playing
    }

    fn config(&self) -> &AudioSinkConfig {
        &self.config
    }
}

/// How long a stream may hold queued audio without playing any before its buffer size is
/// lowered to what is queued.
pub const START_STALL: std::time::Duration = std::time::Duration::from_millis(30);

/// Chooses an output stream's buffer size (the most it holds, in frames).
///
/// AAudio streams that run on the legacy AudioTrack path only start, and restart after an
/// underrun, once the buffer is filled to its buffer size, while a writer that keeps a modest
/// cushion never fills a large one. So: while the stream plays, run at the full capacity (room
/// to absorb bursts); when it has held audio without playing for [`START_STALL`], drop to what
/// is queued so it starts.
#[derive(Debug, Default)]
pub struct BufferSizer {
    last_read: Option<i64>,
    last_progress: Option<std::time::Instant>,
}

impl BufferSizer {
    /// The buffer size to switch to, if it should change. `read` is the stream's frames-read
    /// counter, `queued` the frames waiting to play, `size` and `capacity` the current buffer
    /// size and capacity.
    pub fn update(
        &mut self,
        now: std::time::Instant,
        read: i64,
        queued: u32,
        size: u32,
        capacity: u32,
    ) -> Option<u32> {
        if self.last_read != Some(read) {
            let first = self.last_read.is_none();
            self.last_read = Some(read);
            self.last_progress = Some(now);
            // Playing (or just opened): give it the full capacity once it has played.
            return (!first && size < capacity).then_some(capacity);
        }
        let since = now.saturating_duration_since(self.last_progress.unwrap_or(now));
        if queued > 0 && since >= START_STALL && size > queued {
            self.last_progress = Some(now);
            return Some(queued.max(1));
        }
        None
    }
}

// -------------------------------------------------------------------------------------------------
// AAudio Native Sink for Android / Meta Quest 3 (target_os = "android")
// -------------------------------------------------------------------------------------------------

/// Native low-latency audio output sink using Android NDK's AAudio API.
///
/// Configures AAudio in `AAUDIO_PERFORMANCE_MODE_LOW_LATENCY` with `AAUDIO_SHARING_MODE_EXCLUSIVE`
/// (or `SHARED` fallback), bypassing intermediate Android audio mixer layers and achieving sub-10ms
/// audio output latency on the Meta Quest 3's Snapdragon XR2 Gen 2 DSP audio hardware.
///
/// ### OpenSL ES Fallback Architecture Note
///
/// On older Android platforms (API < 26, such as early Oculus Quest 1 headsets based on Android 7.1 Nougat),
/// low-latency audio output was implemented using OpenSL ES (`libOpenSLES.so`):
/// - A global engine object was created via `slCreateEngine(&engineObject, ...)`.
/// - An output mix was created via `(*engine)->CreateOutputMix(engine, &outputMixObject, ...)`.
/// - An audio player was initialized with `SLDataLocator_AndroidSimpleBufferQueue` and `SLDataFormat_PCM_EX`.
/// - Playout was driven by buffer queue callbacks (`SLAndroidSimpleBufferQueueItf::Enqueue`) exchanging
///   interleaved PCM chunks.
///
/// However, on modern Horizon OS (Android 10 / 12L / 14 on Meta Quest 3, Quest Pro, and Quest 2):
/// 1. OpenSL ES is deprecated by Google and Meta.
/// 2. OpenSL ES on modern Android is merely a legacy wrapper implemented internally on top of AudioFlinger,
///    adding intermediate memory copies, thread hops, and 10–15ms of unavoidable latency.
/// 3. AAudio provides direct Memory-Mapped (MMAP) access to the ALSA DSP hardware buffers, eliminating
///    IPC overhead and guaranteeing glitch-free low latency.
///
/// Therefore, `linux-quest-client` targets AAudio as its first-class audio engine on Quest 3.
/// If OpenSL ES backwards compatibility is ever required for Android API < 26 devices, an `OpenSlSink`
/// implementing the `AudioSink` trait can be added alongside `AAudioSink`.
#[cfg(target_os = "android")]
pub struct AAudioSink {
    config: AudioSinkConfig,
    stream: Option<ndk::audio::AudioStream>,
    is_playing: bool,
    sizer: BufferSizer,
}

#[cfg(target_os = "android")]
impl AAudioSink {
    /// Creates a new uninitialized AAudio sink.
    pub fn new() -> Self {
        Self {
            config: AudioSinkConfig::default(),
            stream: None,
            is_playing: false,
            sizer: BufferSizer::default(),
        }
    }
}

#[cfg(target_os = "android")]
impl Default for AAudioSink {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "android")]
impl AudioSink for AAudioSink {
    fn open(&mut self, config: AudioSinkConfig) -> Result<(), AudioSinkError> {
        use ndk::audio::{
            AudioContentType, AudioDirection, AudioFormat, AudioPerformanceMode, AudioSharingMode,
            AudioStreamBuilder, AudioUsage,
        };

        let performance_mode = if config.low_latency {
            AudioPerformanceMode::LowLatency
        } else {
            AudioPerformanceMode::None
        };

        let builder = AudioStreamBuilder::new()
            .map_err(|e| {
                AudioSinkError::Open(format!("Failed to create AAudioStreamBuilder: {e}"))
            })?
            .direction(AudioDirection::Output)
            .performance_mode(performance_mode)
            .sharing_mode(AudioSharingMode::Exclusive) // Attempt exclusive low-latency mode first
            .format(AudioFormat::PCM_Float)
            .sample_rate(config.sample_rate as i32)
            .channel_count(config.channels as i32)
            .buffer_capacity_in_frames(config.buffer_capacity_frames as i32)
            .usage(AudioUsage::Game)
            .content_type(AudioContentType::Music);

        // Attempt opening the stream, falling back to Shared mode if Exclusive is unavailable
        let stream = match builder.open_stream() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    "Exclusive AAudio stream open failed: {e}. Retrying with AAUDIO_SHARING_MODE_SHARED."
                );
                let fallback_builder = AudioStreamBuilder::new()
                    .map_err(|e| {
                        AudioSinkError::Open(format!("Failed to recreate AAudioStreamBuilder: {e}"))
                    })?
                    .direction(AudioDirection::Output)
                    .performance_mode(performance_mode)
                    .sharing_mode(AudioSharingMode::Shared)
                    .format(AudioFormat::PCM_Float)
                    .sample_rate(config.sample_rate as i32)
                    .channel_count(config.channels as i32)
                    .buffer_capacity_in_frames(config.buffer_capacity_frames as i32)
                    .usage(AudioUsage::Game)
                    .content_type(AudioContentType::Music);

                fallback_builder
                    .open_stream()
                    .map_err(|e| AudioSinkError::Open(format!("Shared AAudio open failed: {e}")))?
            }
        };

        // The buffer size starts small, so the stream starts as soon as a little audio is
        // queued; `BufferSizer` raises it to the capacity once it plays.
        tracing::info!(
            capacity = stream.buffer_capacity_in_frames(),
            buffer_size = stream.buffer_size_in_frames(),
            burst = stream.frames_per_burst(),
            rate = stream.sample_rate(),
            sharing = ?stream.sharing_mode(),
            "AAudio output stream open"
        );
        self.sizer = BufferSizer::default();

        self.stream = Some(stream);
        self.config = config;
        self.is_playing = false;
        Ok(())
    }

    fn start(&mut self) -> Result<(), AudioSinkError> {
        let stream = self.stream.as_mut().ok_or(AudioSinkError::NotReady)?;
        stream
            .request_start()
            .map_err(|e| AudioSinkError::Start(format!("AAudio requestStart failed: {e}")))?;
        self.is_playing = true;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), AudioSinkError> {
        let stream = self.stream.as_mut().ok_or(AudioSinkError::NotReady)?;
        stream
            .request_pause()
            .map_err(|e| AudioSinkError::Pause(format!("AAudio requestPause failed: {e}")))?;
        self.is_playing = false;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AudioSinkError> {
        if let Some(stream) = self.stream.as_mut() {
            let _ = stream.request_stop();
            let _ = stream.request_flush();
        }
        self.is_playing = false;
        self.stream = None;
        Ok(())
    }

    fn write_samples(&mut self, samples: &[f32]) -> Result<usize, AudioSinkError> {
        let stream = self.stream.as_ref().ok_or(AudioSinkError::NotReady)?;
        if !self.is_playing {
            return Err(AudioSinkError::NotReady);
        }
        let read = stream.frames_read();
        let queued = (stream.frames_written() - read).clamp(0, i64::from(u32::MAX)) as u32;
        if let Some(size) = self.sizer.update(
            std::time::Instant::now(),
            read,
            queued,
            stream.buffer_size_in_frames().max(0) as u32,
            stream.buffer_capacity_in_frames().max(0) as u32,
        ) {
            if let Err(e) = stream.set_buffer_size_in_frames(size as i32) {
                tracing::debug!("AAudio setBufferSizeInFrames({size}) failed: {e}");
            }
        }

        let channels = self.config.channels as usize;
        if channels == 0 || samples.is_empty() {
            return Ok(0);
        }

        let num_frames = (samples.len() / channels) as i32;
        // Non-blocking write with 0 timeout to prevent audio glitches from blocking render thread
        let frames_written = unsafe {
            stream
                .write(samples.as_ptr() as *const std::ffi::c_void, num_frames, 0)
                .map_err(|e| AudioSinkError::Write(format!("AAudio write failed: {e}")))?
        };

        Ok((frames_written as usize) * channels)
    }

    fn latency_frames(&self) -> Result<u32, AudioSinkError> {
        let stream = self.stream.as_ref().ok_or(AudioSinkError::NotReady)?;
        let buffer_size = stream.buffer_size_in_frames();
        Ok(buffer_size.max(0) as u32)
    }

    fn queued_frames(&self) -> Option<u32> {
        let stream = self.stream.as_ref()?;
        let queued = stream.frames_written() - stream.frames_read();
        Some(queued.clamp(0, i64::from(u32::MAX)) as u32)
    }

    fn is_playing(&self) -> bool {
        self.is_playing
    }

    fn config(&self) -> &AudioSinkConfig {
        &self.config
    }
}

/// Helper function to create the default audio sink for the current platform.
/// On Android, instantiates `AAudioSink`; on desktop Linux/CI, instantiates `MockAudioSink`.
pub fn create_default_audio_sink() -> Box<dyn AudioSink> {
    #[cfg(target_os = "android")]
    {
        Box::new(AAudioSink::new())
    }
    #[cfg(not(target_os = "android"))]
    {
        Box::new(MockAudioSink::new(AudioSinkConfig::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_audio_sink_lifecycle() {
        let mut sink = MockAudioSink::new(AudioSinkConfig::default());
        assert!(!sink.is_playing());

        sink.open(AudioSinkConfig {
            sample_rate: 48000,
            channels: 2,
            buffer_capacity_frames: 240,
            low_latency: true,
        })
        .unwrap();

        sink.start().unwrap();
        assert!(sink.is_playing());

        // Write stereo samples
        let samples = vec![0.1f32, -0.1, 0.2, -0.2, 0.3, -0.3];
        let written = sink.write_samples(&samples).unwrap();
        assert_eq!(written, 6);
        assert_eq!(sink.total_samples_written(), 6);
        assert_eq!(sink.written_samples(), &samples[..]);

        sink.pause().unwrap();
        assert!(!sink.is_playing());

        // Write while paused returns NotReady
        assert!(matches!(
            sink.write_samples(&samples),
            Err(AudioSinkError::NotReady)
        ));

        sink.stop().unwrap();
        assert!(!sink.is_playing());
    }

    #[test]
    fn test_create_default_audio_sink() {
        let mut sink = create_default_audio_sink();
        sink.open(AudioSinkConfig::default()).unwrap();
        sink.start().unwrap();
        assert!(sink.is_playing());
        sink.stop().unwrap();
    }

    #[test]
    fn test_boxed_audio_sink_delegation() {
        let mut sink: Box<dyn AudioSink> = Box::new(MockAudioSink::new(AudioSinkConfig::default()));
        sink.open(AudioSinkConfig::default()).unwrap();
        sink.start().unwrap();
        assert!(sink.is_playing());
        assert_eq!(sink.latency_frames().unwrap(), 120);
        let samples = vec![0.5f32, -0.5f32];
        assert_eq!(sink.write_samples(&samples).unwrap(), 2);
        sink.pause().unwrap();
        assert!(!sink.is_playing());
        sink.stop().unwrap();
    }

    #[test]
    fn buffer_grows_to_capacity_once_the_stream_plays() {
        let t0 = std::time::Instant::now();
        let mut sizer = BufferSizer::default();
        assert_eq!(sizer.update(t0, 0, 384, 384, 9600), None, "just opened");
        assert_eq!(
            sizer.update(
                t0 + std::time::Duration::from_millis(5),
                192,
                1920,
                384,
                9600
            ),
            Some(9600)
        );
        assert_eq!(
            sizer.update(
                t0 + std::time::Duration::from_millis(10),
                384,
                1920,
                9600,
                9600
            ),
            None
        );
    }

    #[test]
    fn a_stream_holding_audio_without_playing_drops_to_what_is_queued() {
        let t0 = std::time::Instant::now();
        let mut sizer = BufferSizer::default();
        sizer.update(t0, 1000, 0, 9600, 9600);
        // Underran and refilled with 40 ms, but not playing: within the grace period, wait.
        let t = t0 + START_STALL / 2;
        assert_eq!(sizer.update(t, 1000, 1920, 9600, 9600), None);
        let t = t0 + START_STALL;
        assert_eq!(sizer.update(t, 1000, 2400, 9600, 9600), Some(2400));
        // Then it plays and gets the capacity back.
        let t = t + std::time::Duration::from_millis(5);
        assert_eq!(sizer.update(t, 1192, 2208, 2400, 9600), Some(9600));
    }

    #[test]
    fn an_idle_empty_stream_is_left_alone() {
        let t0 = std::time::Instant::now();
        let mut sizer = BufferSizer::default();
        sizer.update(t0, 500, 0, 9600, 9600);
        assert_eq!(sizer.update(t0 + START_STALL * 4, 500, 0, 9600, 9600), None);
    }
}
