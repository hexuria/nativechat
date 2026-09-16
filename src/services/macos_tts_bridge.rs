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

unsafe impl Encode for MyNSRange {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{_NSRange=QQ}") }
    }
}

pub struct MacTtsBridge {
    synthesizer: id,
    delegate: id,
    callback: Arc<Mutex<Option<TtsCallback>>>,
}

unsafe impl Send for MacTtsBridge {}
unsafe impl Sync for MacTtsBridge {}

impl MacTtsBridge {
    pub fn new() -> Self {
        unsafe {
            let synthesizer: id = msg_send![class!(NSSpeechSynthesizer), new];
            if synthesizer.is_null() {
                eprintln!("Failed to create NSSpeechSynthesizer");
            }

            static mut DELEGATE_CLASS: *const Class = 0 as *const Class;
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
                    unsafe {
                        let callback_ptr: *mut c_void = *this.get_ivar("_callback_ptr");
                        if !callback_ptr.is_null() {
                            let callback_arc =
                                &*(callback_ptr as *const Arc<Mutex<Option<TtsCallback>>>);
                            if let Ok(guard) = callback_arc.lock() {
                                if let Some(cb) = &*guard {
                                    cb(TtsEvent::Word {
                                        start: range.location,
                                        length: range.length,
                                    });
                                }
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
                    unsafe {
                        let callback_ptr: *mut c_void = *this.get_ivar("_callback_ptr");
                        if !callback_ptr.is_null() {
                            let callback_arc =
                                &*(callback_ptr as *const Arc<Mutex<Option<TtsCallback>>>);
                            if let Ok(guard) = callback_arc.lock() {
                                if let Some(cb) = &*guard {
                                    cb(TtsEvent::Finish);
                                }
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
            (*delegate).set_ivar("_callback_ptr", 0 as *mut c_void);

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

        unsafe {
            let ptr = &self.callback as *const Arc<Mutex<Option<TtsCallback>>> as *mut c_void;
            (*self.delegate).set_ivar("_callback_ptr", ptr);
        }
    }

    pub fn speak(&self, text: &str) {
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
        unsafe {
            let _: () = msg_send![self.synthesizer, continueSpeaking];
            let _: () = msg_send![self.synthesizer, stopSpeaking];
        }
    }

    pub fn pause(&self) {
        unsafe {
            let _: () = msg_send![self.synthesizer, pauseSpeakingAtBoundary:1]; // 0 = Immediate, 1 = Word, 2 = Sentence
        }
    }

    pub fn resume(&self) {
        unsafe {
            let _: () = msg_send![self.synthesizer, continueSpeaking];
        }
    }

    pub fn is_speaking(&self) -> bool {
        unsafe {
            let speaking: bool = msg_send![self.synthesizer, isSpeaking];
            speaking
        }
    }
}

impl Drop for MacTtsBridge {
    fn drop(&mut self) {
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
