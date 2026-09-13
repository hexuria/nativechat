//! Audio transcription service for extracting word-level timestamps from audio.
//! Uses Gemini's audio understanding capability to transcribe TTS audio
//! and get accurate word timing for highlighting.

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ops::Range;

/// Word timing information extracted from audio transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTiming {
    /// The word text
    pub word: String,
    /// Start time in seconds from beginning of audio
    pub start_time: f32,
    /// End time in seconds from beginning of audio
    pub end_time: f32,
    /// Byte range in the source text (for highlighting)
    pub source_range: Range<usize>,
}

/// Service for transcribing audio and extracting word timestamps
#[derive(Clone)]
pub struct AudioTranscriptionService {
    client: Client,
}

impl AudioTranscriptionService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Transcribe audio samples and extract word-level timestamps.
    ///
    /// # Arguments
    /// * `samples` - f32 audio samples at 24kHz
    /// * `source_text` - The original text that was spoken (for mapping words back to source positions)
    /// * `api_key` - Gemini API key
    ///
    /// # Returns
    /// A vector of WordTiming structs with start/end times and source positions
    pub async fn transcribe_with_timestamps(
        &self,
        samples: &[f32],
        source_text: &str,
        api_key: &str,
    ) -> Result<Vec<WordTiming>> {
        // First, parse source text into words with byte positions
        // This is what we'll highlight - the ACTUAL source text
        let source_words = self.parse_source_words(source_text);
        if source_words.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate audio duration from samples
        let audio_duration = samples.len() as f32 / 24000.0;

        // Try to get word-level timestamps from Gemini
        // If that fails, fall back to proportional distribution
        let timings = match self.get_gemini_timestamps(samples, api_key).await {
            Ok(raw_timings) if !raw_timings.is_empty() => {
                println!(
                    "[Transcription] Got {} raw timings from Gemini",
                    raw_timings.len()
                );
                // Use the transcription's timing to scale our source words
                self.apply_timings_to_source_words(&source_words, &raw_timings, audio_duration)
            }
            _ => {
                println!("[Transcription] Falling back to proportional timing");
                // Fallback: distribute duration proportionally
                self.distribute_proportional_timing(&source_words, audio_duration)
            }
        };

        Ok(timings)
    }

    /// Parse source text into words with their byte positions
    fn parse_source_words(&self, source_text: &str) -> Vec<(String, Range<usize>)> {
        let mut words = Vec::new();
        let mut start = 0;
        let mut in_word = false;

        for (i, c) in source_text.char_indices() {
            if c.is_alphanumeric() || c == '\'' {
                if !in_word {
                    start = i;
                    in_word = true;
                }
            } else if in_word {
                words.push((source_text[start..i].to_string(), start..i));
                in_word = false;
            }
        }
        // Handle last word
        if in_word {
            words.push((source_text[start..].to_string(), start..source_text.len()));
        }
        words
    }

    /// Get raw timestamps from Gemini STT
    async fn get_gemini_timestamps(
        &self,
        samples: &[f32],
        api_key: &str,
    ) -> Result<Vec<RawWordTiming>> {
        let wav_data = self.samples_to_wav(samples)?;
        let audio_base64 = general_purpose::STANDARD.encode(&wav_data);

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key={}",
            api_key
        );

        let payload = json!({
            "contents": [{
                "parts": [
                    {
                        "inlineData": {
                            "mimeType": "audio/wav",
                            "data": audio_base64
                        }
                    },
                    {
                        "text": "Transcribe this audio with precise word-level timestamps. For each word, provide the start time and end time in seconds. Format your response as JSON array with objects containing 'word', 'start', 'end' fields. Only output the JSON array, no other text."
                    }
                ]
            }],
            "generationConfig": {
                "temperature": 0,
                "responseMimeType": "application/json"
            }
        });

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send transcription request")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Transcription API error: {}", error_text));
        }

        let response_json: serde_json::Value = response.json().await?;

        // Extract the transcription from the response
        let text = response_json
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.get(0))
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("[]");

        // Parse the JSON array of word timings
        let raw_timings: Vec<RawWordTiming> =
            serde_json::from_str(text).unwrap_or_else(|_| Vec::new());

        Ok(raw_timings)
    }

    /// Convert f32 samples at 24kHz to WAV format
    fn samples_to_wav(&self, samples: &[f32]) -> Result<Vec<u8>> {
        let sample_rate = 24000u32;
        let bits_per_sample = 16u16;
        let num_channels = 1u16;
        let byte_rate = sample_rate * (bits_per_sample as u32 / 8) * num_channels as u32;
        let block_align = num_channels * (bits_per_sample / 8);
        let data_size = (samples.len() * 2) as u32;
        let file_size = 36 + data_size;

        let mut wav = Vec::with_capacity(44 + samples.len() * 2);

        // RIFF header
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");

        // fmt chunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        wav.extend_from_slice(&1u16.to_le_bytes()); // audio format (PCM)
        wav.extend_from_slice(&num_channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());

        // data chunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());

        // Convert f32 samples to i16
        for &sample in samples {
            let s = (sample * 32768.0).clamp(-32768.0, 32767.0) as i16;
            wav.extend_from_slice(&s.to_le_bytes());
        }

        Ok(wav)
    }

    /// Apply transcribed timings to source words sequentially.
    /// Matched words get exact timing, unmatched words get interpolated.
    fn apply_timings_to_source_words(
        &self,
        source_words: &[(String, Range<usize>)],
        raw_timings: &[RawWordTiming],
        audio_duration: f32,
    ) -> Vec<WordTiming> {
        let mut final_timings = Vec::new();

        let mut raw_idx = 0;
        let mut last_valid_end = 0.0;

        // Step 1: Greedy forward matching
        // Map source words to raw timings where possible
        for (source_start_byte, source_end_byte) in
            source_words.iter().map(|(_, r)| (r.start, r.end))
        {
            let source_word_str = &source_words[final_timings.len()].0;
            let source_lower = source_word_str.to_lowercase();

            let mut matched_timing = None;

            // Look ahead in raw_timings to find a match
            // Limit lookahead to avoid skipping too much if words are missing
            let search_limit = (raw_idx + 5).min(raw_timings.len());

            for j in raw_idx..search_limit {
                let raw_word_clean = raw_timings[j]
                    .word
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase();
                if !raw_word_clean.is_empty() && raw_word_clean == source_lower {
                    matched_timing = Some(&raw_timings[j]);
                    raw_idx = j + 1; // Advance raw_idx past this match
                    break;
                }
            }

            if let Some(timing) = matched_timing {
                final_timings.push(WordTiming {
                    word: source_word_str.clone(),
                    start_time: timing.start,
                    end_time: timing.end,
                    source_range: source_start_byte..source_end_byte,
                });
                last_valid_end = timing.end;
            } else {
                // Gap - push a placeholder with zero duration
                // We will fill this in Step 2
                final_timings.push(WordTiming {
                    word: source_word_str.clone(),
                    start_time: last_valid_end,
                    end_time: last_valid_end,
                    source_range: source_start_byte..source_end_byte,
                });
            }
        }

        // Step 2: Interpolate gaps
        self.interpolate_gaps(&mut final_timings, audio_duration);

        final_timings
    }

    /// Fill zero-duration gaps by distributing available time proportionally
    fn interpolate_gaps(&self, timings: &mut Vec<WordTiming>, total_duration: f32) {
        if timings.is_empty() {
            return;
        }

        let mut i = 0;
        while i < timings.len() {
            // Find a block of zero-duration words
            if timings[i].start_time == timings[i].end_time {
                let block_start_idx = i;
                let start_time = timings[i].start_time;

                // Find end of block and next valid start time
                while i < timings.len() && timings[i].start_time == timings[i].end_time {
                    i += 1;
                }

                let end_time = if i < timings.len() {
                    timings[i].start_time
                } else {
                    total_duration
                };

                // Distribute (end_time - start_time) across [block_start_idx..i]
                let duration = end_time - start_time;
                let count = i - block_start_idx;

                if duration > 0.0 && count > 0 {
                    let total_chars: usize = timings[block_start_idx..i]
                        .iter()
                        .map(|w| w.word.len())
                        .sum();
                    let time_per_char = if total_chars > 0 {
                        duration / total_chars as f32
                    } else {
                        0.0
                    };

                    let mut current = start_time;
                    for j in block_start_idx..i {
                        let w_len = timings[j].word.len();
                        let w_dur = w_len as f32 * time_per_char;
                        // Fallback to even distribution if chars are zero (weird case)
                        let w_dur = if w_dur == 0.0 {
                            duration / count as f32
                        } else {
                            w_dur
                        };

                        timings[j].start_time = current;
                        timings[j].end_time = current + w_dur;
                        current += w_dur;
                    }
                }
            } else {
                i += 1;
            }
        }
    }

    /// Fallback: distribute duration proportionally across all source words
    fn distribute_proportional_timing(
        &self,
        source_words: &[(String, Range<usize>)],
        duration: f32,
    ) -> Vec<WordTiming> {
        let total_chars: usize = source_words.iter().map(|(w, _)| w.len()).sum();
        let time_per_char = if total_chars > 0 {
            duration / total_chars as f32
        } else {
            0.0
        };

        let mut current_time = 0.0;
        let mut timings = Vec::new();

        for (word, range) in source_words {
            let word_duration = word.len() as f32 * time_per_char;
            let end_time = current_time + word_duration;

            timings.push(WordTiming {
                word: word.clone(),
                start_time: current_time,
                end_time: end_time,
                source_range: range.clone(),
            });
            current_time = end_time;
        }

        timings
    }
}

/// Raw word timing from the API response
#[derive(Debug, Deserialize)]
struct RawWordTiming {
    word: String,
    start: f32,
    end: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proportional_timing_covers_duration() {
        let svc = AudioTranscriptionService::new();
        let words = vec![
            ("hello".to_string(), 0..5),
            ("world".to_string(), 6..11),
        ];
        let timings = svc.distribute_proportional_timing(&words, 2.0);
        assert_eq!(timings.len(), 2);
        assert!((timings[0].start_time - 0.0).abs() < f32::EPSILON);
        assert!((timings.last().unwrap().end_time - 2.0).abs() < 0.0001);
        assert_eq!(timings[0].source_range, 0..5);
        assert_eq!(timings[1].source_range, 6..11);
    }

    #[test]
    fn interpolate_fills_zero_duration_gap() {
        let svc = AudioTranscriptionService::new();
        let mut timings = vec![
            WordTiming {
                word: "a".into(),
                start_time: 0.0,
                end_time: 0.0,
                source_range: 0..1,
            },
            WordTiming {
                word: "b".into(),
                start_time: 0.0,
                end_time: 0.0,
                source_range: 2..3,
            },
            WordTiming {
                word: "c".into(),
                start_time: 1.0,
                end_time: 2.0,
                source_range: 4..5,
            },
        ];
        svc.interpolate_gaps(&mut timings, 2.0);
        assert!(timings[0].end_time > timings[0].start_time);
        assert!(timings[1].end_time > timings[1].start_time);
        assert!((timings[1].end_time - 1.0).abs() < 0.0001);
    }
}
