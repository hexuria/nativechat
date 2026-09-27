use coreaudio_sys as sys;
use std::ffi::c_void;
use std::ptr;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

pub struct AudioInput {
    audio_unit: sys::AudioUnit,
    /// The callback's context, handed to the audio unit as its refcon. It is owned through this
    /// raw pointer, not a Box: CoreAudio reads it on its own thread for as long as the unit runs,
    /// and a Box held (or moved) here meanwhile would claim a uniqueness it does not have. Taken
    /// back in Drop, once the unit is disposed of.
    context: *mut InputContext,
}

struct InputContext {
    unit: sys::AudioUnit, // Added to store the AudioUnit reference
    amplitude: Arc<AtomicU32>,
}

impl AudioInput {
    pub fn new(amplitude: Arc<AtomicU32>) -> anyhow::Result<Self> {
        eprintln!("[AudioInput] Creating new VoiceProcessingIO instance (sys)");

        // SAFETY: plain CoreAudio C calls. Every pointer passed in is to a local or to the context,
        // each valid for the call it is passed to. The context is leaked with `Box::into_raw` and
        // handed to the unit as its refcon; nothing else refers to it, and it is taken back with
        // `Box::from_raw` exactly once: in `abandon` if setup fails after that, else in Drop, in
        // both cases only after the unit is disposed of and its callback can no longer run.
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
            let context = Box::into_raw(Box::new(InputContext {
                unit: audio_unit,
                amplitude,
            }));
            // Setup failed after the unit was handed the context. The unit never started, so its
            // callback has not run; dispose of it first, so it cannot, then take the context back.
            let abandon = |step: &str, status: sys::OSStatus| {
                sys::AudioComponentInstanceDispose(audio_unit);
                drop(Box::from_raw(context));
                anyhow::anyhow!("{step}: {status}")
            };

            let callback_struct = sys::AURenderCallbackStruct {
                inputProc: Some(input_callback),
                inputProcRefCon: context.cast(),
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
                return Err(abandon("Failed to set callback", status));
            }

            // 7. Initialize and Start
            let status = sys::AudioUnitInitialize(audio_unit);
            if status != 0 {
                return Err(abandon("Failed to initialize unit", status));
            }

            let status = sys::AudioOutputUnitStart(audio_unit);
            if status != 0 {
                return Err(abandon("Failed to start unit", status));
            }

            Ok(Self {
                audio_unit,
                context,
            })
        }
    }
}

impl Drop for AudioInput {
    fn drop(&mut self) {
        // SAFETY: `audio_unit` is the instance `new` created and started, and it is disposed of
        // exactly once, here. Once it is disposed of its callback cannot run again, so the
        // context `new` leaked with `Box::into_raw` is taken back, also exactly once.
        unsafe {
            sys::AudioOutputUnitStop(self.audio_unit);
            sys::AudioUnitUninitialize(self.audio_unit);
            sys::AudioComponentInstanceDispose(self.audio_unit);
            drop(Box::from_raw(self.context));
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
    // SAFETY: `in_ref_con` is the InputContext `new` leaked with `Box::into_raw`, freed only after
    // the unit is disposed of (see `new` and Drop), so it is alive whenever this runs. The callback
    // only reads it and stores to an atomic, so a shared reference is all it takes.
    unsafe {
        let context = &*(in_ref_con as *const InputContext);

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

        0 // noErr
    }
}
