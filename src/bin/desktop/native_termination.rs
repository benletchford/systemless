//! Defer AppKit termination until the execution owner has flushed saves and
//! destroyed the guest. NSTerminateLater runs a modal loop, so completion must
//! not depend on winit delivering another event-loop callback.

use super::runtime_mailbox::{RuntimeMailbox, RuntimeStatus};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::sel;
use objc2_app_kit::{NSApplication, NSModalPanelRunLoopMode};
use objc2_foundation::{MainThreadMarker, NSRunLoop, NSRunLoopCommonModes, NSTimer};
use std::cell::{Cell, RefCell};
use std::ffi::c_char;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

struct Active {
    mailbox: Arc<RuntimeMailbox>,
    timer: Option<Retained<NSTimer>>,
    pending: bool,
    replied: bool,
}

impl Active {
    fn begin(&mut self) -> bool {
        if self.pending {
            return false;
        }
        self.pending = true;
        self.mailbox.request_shutdown();
        true
    }

    fn completion(&mut self) -> Option<RuntimeStatus> {
        if !self.pending || self.replied {
            return None;
        }
        let status = self.mailbox.status();
        if matches!(status, RuntimeStatus::Stopped { .. }) {
            self.replied = true;
            Some(status)
        } else {
            None
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

#[link(name = "objc")]
extern "C" {
    fn class_addMethod(
        class: *const AnyClass,
        selector: Sel,
        imp: Imp,
        types: *const c_char,
    ) -> Bool;
}

// All supported macOS targets have NSUInteger == usize. These signatures match
// -applicationShouldTerminate: (Q@:@) and the timer action (v@:@). The runtime
// retains these function pointers for the lifetime of the registered class.
unsafe extern "C" fn should_terminate(receiver: &AnyObject, _: Sel, _: &NSApplication) -> usize {
    ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(active) = slot.as_mut() else {
            return 1;
        }; // NSTerminateNow
        if !active.begin() {
            return 2; // NSTerminateLater; keep the existing completion timer.
        }
        // A timer explicitly registered in AppKit's termination modal mode can
        // complete shutdown even while normal winit event delivery is suspended.
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                0.01,
                receiver,
                sel!(systemlessPollRuntimeTermination:),
                None,
                true,
            )
        };
        let run_loop = unsafe { NSRunLoop::mainRunLoop() };
        unsafe {
            run_loop.addTimer_forMode(&timer, NSModalPanelRunLoopMode);
            run_loop.addTimer_forMode(&timer, NSRunLoopCommonModes);
        }
        active.timer = Some(timer);
        2
    })
}

unsafe extern "C" fn poll_termination(_: &AnyObject, _: Sel, _: &NSTimer) {
    let finished = ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(active) = slot.as_mut() else {
            return false;
        };
        let Some(RuntimeStatus::Stopped { error, .. }) = active.completion() else {
            return false;
        };
        if let Some(error) = error {
            eprintln!("[SYSTEMLESS] Runtime stopped during native Quit: {error}");
        }
        if let Some(timer) = active.timer.take() {
            unsafe { timer.invalidate() };
        }
        // Keep pending true until the native termination completes, so winit
        // cannot take over termination or present an already-stopped guest.
        true
    });
    if finished {
        // Release the RefCell borrow before AppKit can synchronously reenter.
        let mtm = MainThreadMarker::new().expect("termination timer runs on main thread");
        unsafe { NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(true) };
    }
}

/// Main-thread lifetime guard. Install before starting the runtime so failure
/// cannot leave a live guest with an unprotected native termination path.
pub(super) struct NativeTermination(PhantomData<Rc<()>>);

impl NativeTermination {
    pub fn install() -> Result<Self, String> {
        let mtm = MainThreadMarker::new().ok_or("native termination requires main thread")?;
        if ACTIVE.with(|slot| slot.borrow().is_some()) {
            return Err("native termination already has an active runtime".into());
        }
        if !INSTALLED.with(Cell::get) {
            let app = NSApplication::sharedApplication(mtm);
            let delegate = unsafe { app.delegate() }.ok_or("AppKit has no event-loop delegate")?;
            // ProtocolObject erases methods but is still an Objective-C object.
            // The retained delegate keeps this shared object reference alive.
            let object = unsafe { &*Retained::as_ptr(&delegate).cast::<AnyObject>() };
            let class = object.class();
            let terminate = sel!(applicationShouldTerminate:);
            let poll = sel!(systemlessPollRuntimeTermination:);
            // Preserve winit's concrete delegate and never replace an existing
            // implementation. A future winit termination hook needs integration,
            // not method swizzling that silently discards its behavior.
            if class.instance_method(terminate).is_some() || class.instance_method(poll).is_some() {
                return Err("AppKit delegate already implements a termination hook".into());
            }
            // SAFETY: Function ABIs and Objective-C encodings match above. The
            // registered class and static functions outlive every application.
            let installed = unsafe {
                class_addMethod(
                    class,
                    poll,
                    std::mem::transmute::<unsafe extern "C" fn(&AnyObject, Sel, &NSTimer), Imp>(
                        poll_termination,
                    ),
                    b"v@:@\0".as_ptr().cast(),
                )
                .as_bool()
                    && class_addMethod(
                        class,
                        terminate,
                        std::mem::transmute::<
                            unsafe extern "C" fn(&AnyObject, Sel, &NSApplication) -> usize,
                            Imp,
                        >(should_terminate),
                        b"Q@:@\0".as_ptr().cast(),
                    )
                    .as_bool()
            };
            if !installed {
                return Err("could not install AppKit termination callbacks".into());
            }
            INSTALLED.with(|installed| installed.set(true));
        }
        Ok(Self(PhantomData))
    }

    pub fn activate(&self, mailbox: Arc<RuntimeMailbox>) {
        ACTIVE.with(|slot| {
            *slot.borrow_mut() = Some(Active {
                mailbox,
                timer: None,
                pending: false,
                replied: false,
            })
        });
    }
}

pub(super) fn pending() -> bool {
    ACTIVE.with(|slot| slot.borrow().as_ref().is_some_and(|active| active.pending))
}

impl Drop for NativeTermination {
    fn drop(&mut self) {
        // Native run-loop callbacks only access this main-thread local state.
        let active = ACTIVE.with(|slot| slot.borrow_mut().take());
        if let Some(mut active) = active {
            if let Some(timer) = active.timer.take() {
                unsafe { timer.invalidate() };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_quit_waits_for_terminal_status_and_replies_once() {
        let mailbox = Arc::new(RuntimeMailbox::new(|| {}));
        let mut active = Active {
            mailbox: mailbox.clone(),
            timer: None,
            pending: false,
            replied: false,
        };
        assert!(active.completion().is_none());
        assert!(active.begin());
        assert!(mailbox.shutdown_requested());
        assert!(!active.begin());
        assert!(active.completion().is_none());
        mailbox.set_status(RuntimeStatus::Ready);
        assert!(active.completion().is_none());
        let stopped = RuntimeStatus::Stopped {
            error: Some("save error".into()),
            instructions: 42,
        };
        mailbox.set_status(stopped.clone());
        assert_eq!(active.completion(), Some(stopped.clone()));
        assert!(active.completion().is_none());
        // Native polling must not consume the regular host's terminal status.
        assert_eq!(mailbox.poll().status, stopped);
    }
}
