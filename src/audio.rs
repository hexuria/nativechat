use crate::services::gemini_client::GeminiLiveClient;
use base64::{Engine as _, engine::general_purpose};
use coreaudio_sys as sys;
use std::ffi::c_void;
use std::ptr;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

pub struct AudioInput {
    audio_unit: sys::AudioUnit,
    // Keep context alive
    _context: Box<InputContext>,
}

struct InputContext {
    unit: sys::AudioUnit, // Added to store the AudioUnit reference
    amplitude: Arc<AtomicU32>,
    gemini_client: Option<GeminiLiveClient>,
    buffer: Vec<i16>,
}

impl AudioInput {
    pub fn new(
        amplitude: Arc<AtomicU32>,
        gemini_client: Option<GeminiLiveClient>,
    ) -> anyhow::Result<Self> {
        println!(
            "[AudioInput] Creating new VoiceProcessingIO instance (sys). Has client: {}",
            gemini_client.is_some()
        );

        unsafe {
            // 1. Describe the Audio Component (VoiceProcessingIO)
            let desc = sys::AudioComponentDescription {
                componentType: sys::kAudioUnitType_Output,
                componentSubType: sys::kAudioUnitSubType_VoiceProcessingIO,
                componentManufacturer: sys::kAudioUnitManufacturer_Apple,
                componentFlags: 0,
                componentFlagsMask: 0,
            };

            // 2. Find and Open Component
            let comp = sys::AudioComponentFindNext(ptr::null_mut(), &desc);
            if comp.is_null() {
                return Err(anyhow::anyhow!(
                    "Failed to find VoiceProcessingIO component"
                ));
            }

            let mut audio_unit: sys::AudioUnit = ptr::null_mut();
            let status = sys::AudioComponentInstanceNew(comp, &mut audio_unit);
            if status != 0 {
                return Err(anyhow::anyhow!("Failed to open component: {}", status));
            }

            // 3. Enable Input (Scope: Input, Element: 1)
            let enable_input: u32 = 1;
            let status = sys::AudioUnitSetProperty(
                audio_unit,
                sys::kAudioOutputUnitProperty_EnableIO,
                sys::kAudioUnitScope_Input,
                1, // Input Element
                &enable_input as *const _ as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
            if status != 0 {
                return Err(anyhow::anyhow!("Failed to enable input: {}", status));
            }

            // 4. Enable Output (Scope: Output, Element: 0) - Required for AEC
            let enable_output: u32 = 1;
            let status = sys::AudioUnitSetProperty(
                audio_unit,
                sys::kAudioOutputUnitProperty_EnableIO,
                sys::kAudioUnitScope_Output,
                0, // Output Element
                &enable_output as *const _ as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
            if status != 0 {
                return Err(anyhow::anyhow!("Failed to enable output: {}", status));
            }

            // 5. Set Format (48kHz Mono Float)
            let sample_rate = 48000.0;
            let stream_format = sys::AudioStreamBasicDescription {
                mSampleRate: sample_rate,
                mFormatID: sys::kAudioFormatLinearPCM,
                mFormatFlags: sys::kAudioFormatFlagIsFloat
                    | sys::kAudioFormatFlagIsPacked
                    | sys::kAudioFormatFlagIsNonInterleaved,
                mFramesPerPacket: 1,
                mChannelsPerFrame: 1,
                mBitsPerChannel: 32,
                mBytesPerPacket: 4,
                mBytesPerFrame: 4,
                mReserved: 0,
            };

            // Set format on Input Element's Output Scope (where we read from)
            let status = sys::AudioUnitSetProperty(
                audio_unit,
                sys::kAudioUnitProperty_StreamFormat,
                sys::kAudioUnitScope_Output,
                1, // Input Element
                &stream_format as *const _ as *const c_void,
                std::mem::size_of::<sys::AudioStreamBasicDescription>() as u32,
            );
            if status != 0 {
                return Err(anyhow::anyhow!("Failed to set input format: {}", status));
            }

            // Also set format on Output Element's Input Scope (where we write to, if we were playing audio)
            // This is good practice for VPIO to match sample rates.
            let status = sys::AudioUnitSetProperty(
                audio_unit,
                sys::kAudioUnitProperty_StreamFormat,
                sys::kAudioUnitScope_Input,
                0, // Output Element
                &stream_format as *const _ as *const c_void,
                std::mem::size_of::<sys::AudioStreamBasicDescription>() as u32,
            );
            if status != 0 {
                return Err(anyhow::anyhow!("Failed to set output format: {}", status));
            }

            // 6. Setup Callback Context
            let mut context = Box::new(InputContext {
                unit: ptr::null_mut(), // Initialize as null, update later
                amplitude,
                gemini_client,
                buffer: Vec::with_capacity(1600),
            });

            // Update unit in context
            context.unit = audio_unit;

            let callback_struct = sys::AURenderCallbackStruct {
                inputProc: Some(input_callback),
                inputProcRefCon: &*context as *const _ as *mut c_void,
            };

            let status = sys::AudioUnitSetProperty(
                audio_unit,
                sys::kAudioOutputUnitProperty_SetInputCallback,
                sys::kAudioUnitScope_Global,
                1, // Input Element
                &callback_struct as *const _ as *const c_void,
                std::mem::size_of::<sys::AURenderCallbackStruct>() as u32,
            );
            if status != 0 {
                return Err(anyhow::anyhow!("Failed to set callback: {}", status));
            }

            // 7. Initialize and Start
            let status = sys::AudioUnitInitialize(audio_unit);
            if status != 0 {
                return Err(anyhow::anyhow!("Failed to initialize unit: {}", status));
            }

            let status = sys::AudioOutputUnitStart(audio_unit);
            if status != 0 {
                return Err(anyhow::anyhow!("Failed to start unit: {}", status));
            }

            Ok(Self {
                audio_unit,
                _context: context,
            })
        }
    }
}

impl Drop for AudioInput {
    fn drop(&mut self) {
        unsafe {
            sys::AudioOutputUnitStop(self.audio_unit);
            sys::AudioUnitUninitialize(self.audio_unit);
            sys::AudioComponentInstanceDispose(self.audio_unit);
        }
    }
}

extern "C" fn input_callback(
    in_ref_con: *mut c_void,
    io_action_flags: *mut sys::AudioUnitRenderActionFlags,
    in_time_stamp: *const sys::AudioTimeStamp,
    in_bus_number: u32,
    in_number_frames: u32,
    _io_data: *mut sys::AudioBufferList, // This is ignored for input callbacks
) -> sys::OSStatus {
    unsafe {
        let context = &mut *(in_ref_con as *mut InputContext);

        // Allocate buffer for data
        let mut data = vec![0.0f32; in_number_frames as usize];
        let mut buffer_list = sys::AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [sys::AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: in_number_frames * 4, // 4 bytes per float
                mData: data.as_mut_ptr() as *mut c_void,
            }],
        };

        // Call Render to pull data from microphone (with AEC applied)
        // The bus number for input is 1.
        let status = sys::AudioUnitRender(
            context.unit, // Use the stored unit from context
            io_action_flags,
            in_time_stamp,
            in_bus_number, // This should be 1 for input
            in_number_frames,
            &mut buffer_list,
        );

        if status != 0 {
            // eprintln!("AudioUnitRender failed in callback: {}", status);
            return status;
        }

        // Process Audio
        let samples = &data;

        // 1. Amplitude
        let mut sum_sq: f32 = 0.0;
        for &sample in samples {
            sum_sq += sample * sample;
        }
        let rms = (sum_sq / samples.len() as f32).sqrt();
        let boosted = (rms * 5.0).min(1.0);
        context
            .amplitude
            .store(boosted.to_bits(), Ordering::Relaxed);

        // 2. Gemini Client
        if let Some(client) = &context.gemini_client {
            // Resample 48k -> 16k (Decimate by 3)
            for chunk in samples.chunks(3) {
                let sum: f32 = chunk.iter().sum();
                let avg = sum / chunk.len() as f32;

                let s = avg.clamp(-1.0, 1.0);
                let val = (s * 32767.0) as i16;
                context.buffer.push(val);
            }

            if context.buffer.len() >= 1600 {
                // 1600 samples = 100ms at 16kHz
                let mut pcm_bytes = Vec::with_capacity(context.buffer.len() * 2); // 2 bytes per i16
                for val in &context.buffer {
                    pcm_bytes.extend_from_slice(&val.to_le_bytes());
                }

                let base64_audio = general_purpose::STANDARD.encode(&pcm_bytes);
                client.send_audio(base64_audio);
                context.buffer.clear();
            }
        }

        0 // noErr
    }
}
