use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use url::Url;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::Mutex;

const GEMINI_URL: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

enum AudioCommand {
    Samples(Vec<f32>),
    Stop,
}

struct AudioOutput {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    is_ai_speaking: Arc<AtomicBool>,
}

impl AudioOutput {
    fn new(is_ai_speaking: Arc<AtomicBool>, ai_amplitude: Arc<AtomicU32>) -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device().context("No output device")?;
        let config = device.default_output_config()?;
        let _sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let buffer_clone = buffer.clone();
        let is_ai_speaking_clone = is_ai_speaking.clone();
        let ai_amplitude_clone = ai_amplitude.clone();

        let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &_| {
                let mut buf = buffer_clone.lock().unwrap();
                let has_samples = !buf.is_empty();
                is_ai_speaking_clone.store(has_samples, Ordering::Relaxed);

                for frame in data.chunks_mut(channels) {
                    if let Some(sample) = buf.pop_front() {
                        for sample_out in frame.iter_mut() {
                            *sample_out = sample;
                        }
                    } else {
                        for sample_out in frame.iter_mut() {
                            *sample_out = 0.0;
                        }
                    }
                }

                // Calculate RMS for Visualizer
                let mut sum_sq = 0.0;
                for sample in data.iter() {
                    sum_sq += sample * sample;
                }
                let rms = (sum_sq / data.len() as f32).sqrt();

                // Apply logarithmic scaling for more natural visualization
                // This prevents bars from maxing out too easily
                let compressed = if rms > 0.0 {
                    // Log scaling: log(1 + x*k) / log(1 + k) where k controls sensitivity
                    let k = 10.0;
                    ((1.0 + rms * k).ln() / (1.0 + k).ln()).min(1.0)
                } else {
                    0.0
                };

                if has_samples {
                    ai_amplitude_clone.store(compressed.to_bits(), Ordering::Relaxed);
                } else {
                    ai_amplitude_clone.store(0, Ordering::Relaxed);
                    is_ai_speaking_clone.store(false, Ordering::Relaxed);
                }
            },
            err_fn,
            None,
        )?;
        stream.play()?;

        Ok(Self {
            _stream: stream,
            buffer,
            is_ai_speaking,
        })
    }

    fn process_command(&self, cmd: AudioCommand) {
        let mut buf = self.buffer.lock().unwrap();
        match cmd {
            AudioCommand::Samples(samples) => {
                // Simple 2x upsampling (24k -> 48k)
                for &s in &samples {
                    buf.push_back(s);
                    buf.push_back(s);
                }
                // Signal that we have data
                self.is_ai_speaking.store(true, Ordering::Relaxed);
            }
            AudioCommand::Stop => {
                buf.clear();
                self.is_ai_speaking.store(false, Ordering::Relaxed);
            }
        }
    }
}

use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("Failed to create Tokio runtime"))
}

#[derive(Clone)]
pub struct GeminiLiveClient {
    tx: mpsc::UnboundedSender<String>,
    is_connected: Arc<AtomicBool>,
    #[allow(dead_code)]
    ai_amplitude: Arc<AtomicU32>,
    #[allow(dead_code)]
    audio_tx: mpsc::UnboundedSender<AudioCommand>,
    is_ai_speaking: Arc<AtomicBool>,
}

impl GeminiLiveClient {
    pub fn connect(api_key: String, ai_amplitude: Arc<AtomicU32>) -> anyhow::Result<Self> {
        let runtime = get_runtime();

        // Log API Key (Masked)
        if api_key.len() > 10 {
            println!(
                "[GeminiClient] Using API Key: {}...{}",
                &api_key[0..5],
                &api_key[api_key.len() - 5..]
            );
        } else {
            println!("[GeminiClient] Using API Key: (too short to mask)");
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let (audio_tx, mut audio_rx) = mpsc::unbounded_channel::<AudioCommand>();
        let is_connected = Arc::new(AtomicBool::new(true));
        let is_connected_clone = is_connected.clone();
        let ai_amplitude_clone = ai_amplitude.clone();
        let audio_tx_clone = audio_tx.clone();

        let is_ai_speaking = Arc::new(AtomicBool::new(false));
        let is_ai_speaking_clone = is_ai_speaking.clone();

        runtime.spawn(async move {
            let url = match Url::parse_with_params(GEMINI_URL, &[("key", &api_key)]) {
                Ok(u) => u,
                Err(e) => {
                    eprintln!("[GeminiClient] URL parse error: {}", e);
                    return;
                }
            };

            println!("[GeminiClient] Connecting to {}...", url);
            let ws_stream = match connect_async(url).await {
                Ok((s, _)) => s,
                Err(e) => {
                    eprintln!("[GeminiClient] Connection failed: {}", e);
                    is_connected_clone.store(false, Ordering::Relaxed);
                    return;
                }
            };

            println!("[GeminiClient] WebSocket connected");
            let (mut write, mut read) = ws_stream.split();

            // Audio Output Thread
            let ai_amp_for_output = ai_amplitude_clone.clone();
            std::thread::spawn(move || {
                let audio_output = match AudioOutput::new(is_ai_speaking_clone, ai_amp_for_output) {
                    Ok(out) => out,
                    Err(e) => {
                        eprintln!("Failed to init audio output: {}", e);
                        return;
                    }
                };

                // Keep thread alive and process samples
                while let Some(cmd) = audio_rx.blocking_recv() {
                    audio_output.process_command(cmd);
                }
            });

            // Send Setup Message
            let setup_msg = json!({
                "setup": {
                    "model": "models/gemini-2.5-flash-native-audio-preview-09-2025",
                    "generationConfig": {
                        "responseModalities": ["AUDIO"],
                        "speechConfig": {
                            "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": "Fenrir" } }
                        }
                    }
                }
            });

            println!("[GeminiClient] Sending setup message: {}", setup_msg);
            if let Err(e) = write
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    setup_msg.to_string(),
                ))
                .await
            {
                eprintln!("[GeminiClient] Failed to send setup: {}", e);
                return;
            }
            println!("[GeminiClient] Setup message sent");

            // Wait for SetupComplete
            println!("[GeminiClient] Waiting for SetupComplete...");
            let mut setup_complete = false;
            while let Some(msg) = read.next().await {
                // println!("[GeminiClient] Received raw message: {:?}", msg);
                let text_msg = match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => Some(text),
                    Ok(tokio_tungstenite::tungstenite::Message::Binary(bin)) => {
                        String::from_utf8(bin).ok()
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(close)) => {
                        eprintln!("[GeminiClient] Connection closed during setup: {:?}", close);
                        is_connected_clone.store(false, Ordering::Relaxed);
                        return;
                    }
                    Err(e) => {
                        eprintln!("[GeminiClient] Error during setup: {}", e);
                        is_connected_clone.store(false, Ordering::Relaxed);
                        return;
                    }
                    _ => None,
                };

                if let Some(text) = text_msg {
                    println!("[GeminiClient] Handshake received text: {:.50}...", text);
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text)
                        && parsed.get("setupComplete").is_some()
                    {
                        println!("[GeminiClient] SetupComplete received!");
                        setup_complete = true;
                        break;
                    }
                }
            }

            if !setup_complete {
                eprintln!("[GeminiClient] Setup failed or incomplete");
                is_connected_clone.store(false, Ordering::Relaxed);
                return;
            }

            // Writer Task
            tokio::spawn(async move {
                println!("[GeminiClient] Writer task started");
                while let Some(msg) = rx.recv().await {
                    // println!("[GeminiClient] Sending message to WebSocket (len: {})", msg.len());
                    if write
                        .send(tokio_tungstenite::tungstenite::Message::Text(msg))
                        .await
                        .is_err()
                    {
                        eprintln!("[GeminiClient] Failed to send message to WebSocket");
                        break;
                    }
                }
                println!("[GeminiClient] Writer task finished");
                let _ = write.close().await;
            });

            // Reader Task
            let ai_amp_clone = ai_amplitude_clone.clone();
            let audio_tx_clone = audio_tx_clone.clone();

            tokio::spawn(async move {
                println!("[GeminiClient] Reader task started");
                while let Some(msg) = read.next().await {
                    let text_msg = match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => Some(text),
                        Ok(tokio_tungstenite::tungstenite::Message::Binary(bin)) => {
                            String::from_utf8(bin).ok()
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(close)) => {
                            eprintln!("[GeminiClient] Connection closed by server: {:?}", close);
                            is_connected_clone.store(false, Ordering::Relaxed);
                            break;
                        }
                        Err(e) => {
                            eprintln!("[GeminiClient] Error receiving message: {}", e);
                            break;
                        }
                        _ => None,
                    };

                    if let Some(text) = text_msg {
                        // println!("[GeminiClient] Received text: {:.50}...", text);
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            // 1. Check for Interruption
                            if let Some(server_content) = parsed.get("serverContent")
                                && let Some(interrupted) =
                                    server_content.get("interrupted").and_then(|v| v.as_bool())
                                && interrupted
                            {
                                println!("[GeminiClient] Interruption detected! Stopping audio.");
                                let _ = audio_tx_clone.send(AudioCommand::Stop);
                                ai_amp_clone.store(0, Ordering::Relaxed);
                            }

                            // 2. Handle Audio Data
                            if let Some(data) =
                                parsed.pointer("/serverContent/modelTurn/parts/0/inlineData/data")
                            {
                                println!("[GeminiClient] Received audio data!");
                                if let Some(base64_str) = data.as_str()
                                    && let Ok(bytes) = general_purpose::STANDARD.decode(base64_str)
                                {
                                    // PCM 16-bit LE -> f32
                                    let mut samples = Vec::with_capacity(bytes.len() / 2);
                                    // let mut sum_sq = 0.0; // Removed calculation here
                                    for chunk in bytes.chunks_exact(2) {
                                        let sample = i16::from_le_bytes([chunk[0], chunk[1]])
                                            as f32
                                            / 32768.0;
                                        samples.push(sample);
                                        // sum_sq += sample * sample; // Removed
                                    }

                                    // Push to Audio Output
                                    if !samples.is_empty() {
                                        let _ = audio_tx_clone.send(AudioCommand::Samples(samples));
                                    }
                                }
                            }
                        }
                    }
                }
                println!("[GeminiClient] Reader task finished");
                ai_amp_clone.store(0, Ordering::Relaxed);
                is_connected_clone.store(false, Ordering::Relaxed);
            });
        });

        Ok(Self {
            tx,
            is_connected,
            ai_amplitude,
            audio_tx,
            is_ai_speaking,
        })
    }
    pub fn send_audio(&self, base64_audio: String) {
        if !self.is_connected.load(Ordering::Relaxed) {
            println!("[GeminiClient] Not connected, dropping audio");
            return;
        }

        // Half-duplex: Drop input audio if AI is speaking to prevent echo
        if self.is_ai_speaking.load(Ordering::Relaxed) {
            // println!("[GeminiClient] AI is speaking, dropping input audio to prevent echo");
            return;
        }

        println!("[GeminiClient] Queuing audio chunk");

        let msg = json!({
            "realtimeInput": {
                "mediaChunks": [{
                    "mimeType": "audio/pcm;rate=16000",
                    "data": base64_audio
                }]
            }
        });

        if let Err(e) = self.tx.send(msg.to_string()) {
            eprintln!("[GeminiClient] Failed to queue audio: {}", e);
        }
    }

    pub fn disconnect(&self) {
        self.is_connected.store(false, Ordering::Relaxed);
        // Channel drop will close writer
    }
}
