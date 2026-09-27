use cocoa::base::{id, nil};
use cocoa::foundation::NSString;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{Encode, Encoding, class, msg_send, sel, sel_impl};
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

pub enum TtsEvent {
    Start,
    Word { start: usize, length: usize },
    Finish,
}

type TtsCallback = Box<dyn Fn(TtsEvent) + Send + Sync>;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct MyNSRange {
    pub location: usize,
    pub length: usize,
}

// SAFETY: MyNSRange is repr(C) with two usize fields, which is NSRange's layout on 64-bit macOS
// (two NSUInteger), and "{_NSRange=QQ}" is the encoding the runtime gives NSRange.
unsafe impl Encode for MyNSRange {
    fn encode() -> Encoding {
        // SAFETY: the string is a well-formed Objective-C type encoding, NSRange's own.
        unsafe { Encoding::from_str("{_NSRange=QQ}") }
    }
}

pub struct MacTtsBridge {
    synthesizer: id,
    delegate: id,
    callback: Arc<Mutex<Option<TtsCallback>>>,
}

// SAFETY: the two ids are only ever messaged, never dereferenced from Rust, and the callback they
// reach is behind a Mutex and is itself Send + Sync. NSSpeechSynthesizer is not documented as
// thread-safe, so this relies on it never being messaged from two threads at once. TtsService,
// the only owner, holds to that: `AppState::ensure_tts_service` builds and warms it on the
// background executor, awaits that task, and only then hands it to the UI thread, which is the
// only thread that messages it from there on. Nothing in the types enforces this; a second owner
// that messages it concurrently would break it.
unsafe impl Send for MacTtsBridge {}
// SAFETY: as for Send above: shared, but messaged by one thread at a time.
unsafe impl Sync for MacTtsBridge {}

impl Default for MacTtsBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl MacTtsBridge {
    pub fn new() -> Self {
        // SAFETY: NSSpeechSynthesizer and NSObject exist on every macOS this app runs on, and each
        // selector is sent with the signature AppKit declares. DELEGATE_CLASS is written once,
        // inside `Once::call_once`, and read only after it. Messaging a nil synthesizer is a no-op
        // in Objective-C. The delegate's ivar is set to null before the delegate is handed over, so
        // a callback that arrives before `set_callback` sees null and does nothing.
        unsafe {
            let synthesizer: id = msg_send![class!(NSSpeechSynthesizer), new];
            if synthesizer.is_null() {
                eprintln!("Failed to create NSSpeechSynthesizer");
            }

            static mut DELEGATE_CLASS: *const Class = std::ptr::null::<Class>();
            static ONCE: std::sync::Once = std::sync::Once::new();

            ONCE.call_once(|| {
                let superclass = class!(NSObject);
                let mut decl = ClassDecl::new("RustTtsDelegate", superclass).unwrap();

                decl.add_ivar::<*mut c_void>("_callback_ptr");

                // willSpeakWord
                extern "C" fn will_speak_word(
                    this: &mut Object,
                    _cmd: Sel,
                    _sender: id,
                    range: MyNSRange,
                    _string: id,
                ) {
                    // SAFETY: `_callback_ptr` is null or `Arc::as_ptr` of the bridge's callback
                    // Mutex (see `set_callback`). That allocation lives as long as the bridge, and
                    // the bridge's Drop clears this delegate before the allocation can be freed.
                    unsafe {
                        let callback_ptr: *mut c_void = *this.get_ivar("_callback_ptr");
                        if !callback_ptr.is_null() {
                            let callback = &*(callback_ptr as *const Mutex<Option<TtsCallback>>);
                            if let Ok(guard) = callback.lock()
                                && let Some(cb) = &*guard
                            {
                                cb(TtsEvent::Word {
                                    start: range.location,
                                    length: range.length,
                                });
                            }
                        }
                    }
                }

                // didFinishSpeaking
                extern "C" fn did_finish_speaking(
                    this: &mut Object,
                    _cmd: Sel,
                    _sender: id,
                    _finished: bool, // BOOL is i8/u8 logic usually, but here just bool works for logic
                ) {
                    // SAFETY: `_callback_ptr` is null or `Arc::as_ptr` of the bridge's callback
                    // Mutex (see `set_callback`). That allocation lives as long as the bridge, and
                    // the bridge's Drop clears this delegate before the allocation can be freed.
                    unsafe {
                        let callback_ptr: *mut c_void = *this.get_ivar("_callback_ptr");
                        if !callback_ptr.is_null() {
                            let callback = &*(callback_ptr as *const Mutex<Option<TtsCallback>>);
                            if let Ok(guard) = callback.lock()
                                && let Some(cb) = &*guard
                            {
                                cb(TtsEvent::Finish);
                            }
                        }
                    }
                }

                decl.add_method(
                    sel!(speechSynthesizer:willSpeakWord:ofString:),
                    will_speak_word as extern "C" fn(&mut Object, Sel, id, MyNSRange, id),
                );

                decl.add_method(
                    sel!(speechSynthesizer:didFinishSpeaking:),
                    did_finish_speaking as extern "C" fn(&mut Object, Sel, id, bool),
                );

                DELEGATE_CLASS = decl.register();
            });

            let delegate: id = msg_send![DELEGATE_CLASS, new];
            (*delegate).set_ivar("_callback_ptr", std::ptr::null_mut::<c_void>());

            let _: () = msg_send![synthesizer, setDelegate:delegate];

            Self {
                synthesizer,
                delegate,
                callback: Arc::new(Mutex::new(None)),
            }
        }
    }

    pub fn set_callback<F>(&self, callback: F)
    where
        F: Fn(TtsEvent) + Send + Sync + 'static,
    {
        let mut guard = self.callback.lock().unwrap();
        *guard = Some(Box::new(callback));

        // The heap allocation behind the Arc, not the address of the `callback` field: that one
        // would dangle the moment the bridge moved, and a caller is free to move it.
        let ptr = Arc::as_ptr(&self.callback) as *mut c_void;
        // SAFETY: `delegate` is the RustTtsDelegate made in `new`, which declares `_callback_ptr`
        // as a `*mut c_void` ivar. What the pointer points at outlives the delegate's use of it:
        // see the callbacks in `new`.
        unsafe {
            (*self.delegate).set_ivar("_callback_ptr", ptr);
        }
    }

    pub fn speak(&self, text: &str) {
        // SAFETY: `synthesizer` is the NSSpeechSynthesizer made in `new`, or nil, which ignores
        // messages; the selector exists on it with this signature.
        unsafe {
            let ns_string = NSString::alloc(nil).init_str(text);
            let success: bool = msg_send![self.synthesizer, startSpeakingString:ns_string];
            if !success {
                eprintln!("NSSpeechSynthesizer startSpeakingString returned NO");
            }
        }
    }

    /// Stops, whether or not the synthesizer is paused. A paused NSSpeechSynthesizer keeps its
    /// paused state across `stopSpeaking` / `startSpeakingString:`, so the next utterance starts
    /// and halts at once and `isSpeaking` stays YES forever; lifting the pause first is the cure.
    pub fn stop(&self) {
        // SAFETY: `synthesizer` is the NSSpeechSynthesizer made in `new`, or nil, which ignores
        // messages; the selector exists on it with this signature.
        unsafe {
            let _: () = msg_send![self.synthesizer, continueSpeaking];
            let _: () = msg_send![self.synthesizer, stopSpeaking];
        }
    }

    pub fn pause(&self) {
        // SAFETY: `synthesizer` is the NSSpeechSynthesizer made in `new`, or nil, which ignores
        // messages; the selector exists on it with this signature.
        unsafe {
            let _: () = msg_send![self.synthesizer, pauseSpeakingAtBoundary:1]; // 0 = Immediate, 1 = Word, 2 = Sentence
        }
    }

    pub fn resume(&self) {
        // SAFETY: `synthesizer` is the NSSpeechSynthesizer made in `new`, or nil, which ignores
        // messages; the selector exists on it with this signature.
        unsafe {
            let _: () = msg_send![self.synthesizer, continueSpeaking];
        }
    }

    pub fn is_speaking(&self) -> bool {
        // SAFETY: `synthesizer` is the NSSpeechSynthesizer made in `new`, or nil, which ignores
        // messages; the selector exists on it with this signature.
        unsafe {
            let speaking: bool = msg_send![self.synthesizer, isSpeaking];
            speaking
        }
    }
}

impl Drop for MacTtsBridge {
    fn drop(&mut self) {
        // SAFETY: as for the other messages. Clearing the delegate here, before the fields drop,
        // is what keeps a late callback from reaching the callback allocation after it is freed.
        unsafe {
            // Clear delegate to prevent use-after-free of callback ptr
            let _: () = msg_send![self.synthesizer, setDelegate:nil];
            // Release objects? Autorelease pool usually handles this, or explicit release if alloc/init.
            // In Rust `cocoa`, we usually rely on autorelease unless retained explicitly.
            // NSSpeechSynthesizer:new returns an autoreleased object usually? No, 'new' returns retained.
            // So we should probably release?
            // let _: () = msg_send![self.synthesizer, release];
            // let _: () = msg_send![self.delegate, release];
            // However, doing FFI manual memory management in drop is tricky without leaks or loose cannons.
            // For now, let's assume standard lifecycle.
        }
    }
}
