use crate::services::macos_tts_bridge::{MacTtsBridge, TtsEvent};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

#[derive(Clone)]
pub struct TtsService {
    #[cfg(target_os = "macos")]
    native_provider: Option<Arc<MacTtsBridge>>,
    native_completion_notify: Arc<Notify>,
    pub native_paused: Arc<AtomicBool>,
    last_word_index: Arc<AtomicUsize>,
    last_word_length: Arc<AtomicUsize>,
    native_generation: Arc<AtomicU64>,
}

impl TtsService {
    pub fn new() -> Self {
        let native_completion = Arc::new(Notify::new());
        let last_word_index = Arc::new(AtomicUsize::new(0));
        let last_word_length = Arc::new(AtomicUsize::new(0));
        let native_generation = Arc::new(AtomicU64::new(0));

        let native_provider = if cfg!(target_os = "macos") {
            let bridge = Arc::new(MacTtsBridge::new());
            let completion = native_completion.clone();
            let index = last_word_index.clone();
            let len = last_word_length.clone();

            bridge.set_callback(move |event| match event {
                TtsEvent::Start => {
                    index.store(0, Ordering::SeqCst);
                    len.store(0, Ordering::SeqCst);
                }
                TtsEvent::Word { start, length } => {
                    index.store(start, Ordering::SeqCst);
                    len.store(length, Ordering::SeqCst);
                }
                TtsEvent::Finish => {
                    completion.notify_waiters();
                    len.store(0, Ordering::SeqCst);
                }
            });

            Some(bridge)
        } else {
            None
        };

        Self {
            native_provider,
            native_completion_notify: native_completion,
            native_paused: Arc::new(AtomicBool::new(false)),
            last_word_index,
            last_word_length,
            native_generation,
        }
    }

    pub fn get_active_word_range(&self) -> Option<std::ops::Range<usize>> {
        let start = self.last_word_index.load(Ordering::SeqCst);
        let len = self.last_word_length.load(Ordering::SeqCst);
        if len > 0 {
            Some(start..start + len)
        } else {
            None
        }
    }

    pub fn warm_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            bridge.speak("\u{00a0}");
            bridge.stop();
        }
    }

    pub fn start_speaking_native(&self, text: &str, _message_id: &str) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            self.native_generation.fetch_add(1, Ordering::SeqCst);
            bridge.stop();
            self.native_paused.store(false, Ordering::SeqCst);
            self.last_word_index.store(0, Ordering::SeqCst);
            self.last_word_length.store(0, Ordering::SeqCst);
            bridge.speak(text);
            return true;
        }

        false
    }

    pub fn pause_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            bridge.pause();
            self.native_paused.store(true, Ordering::SeqCst);
        }
    }

    pub fn resume_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            bridge.resume();
            self.native_paused.store(false, Ordering::SeqCst);
        }
    }

    pub fn stop_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            self.native_generation.fetch_add(1, Ordering::SeqCst);
            bridge.stop();
            self.native_paused.store(false, Ordering::SeqCst);
            self.last_word_length.store(0, Ordering::SeqCst);
            self.native_completion_notify.notify_waiters();
        }
    }

    pub async fn wait_until_finished_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            let started = self.native_generation.load(Ordering::SeqCst);
            loop {
                if self.native_generation.load(Ordering::SeqCst) != started {
                    return;
                }
                let speaking = bridge.is_speaking();
                let paused = self.native_paused.load(Ordering::SeqCst);
                if !speaking && !paused {
                    return;
                }
                tokio::select! {
                    _ = self.native_completion_notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
        }
    }
}
