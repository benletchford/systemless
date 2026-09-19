//! Wait for a DXGI queue slot without blocking the window/input thread.
//!
//! Only the wait handles cross threads; D3D/DXGI calls stay on the window
//! thread. Each arm produces at most one notification, even if the supplied
//! event stays signaled. Drop interrupts either wait and joins the worker
//! before closing the borrowed DXGI handle.
use std::sync::{
    atomic::{AtomicU8, Ordering},
    mpsc::{self, SyncSender},
    Arc,
};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE},
};

const WAITING: u8 = 0;
const READY: u8 = 1;
const FAILED: u8 = 2;

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub struct FrameWaiter {
    state: Arc<AtomicU8>,
    arm: Option<SyncSender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
    stop: OwnedHandle,
}

impl FrameWaiter {
    /// `ready` must remain open until this object has been dropped.
    pub fn new(ready: HANDLE, notify: impl Fn() -> bool + Send + 'static) -> Result<Self, String> {
        let stop = OwnedHandle(
            unsafe { CreateEventW(None, true, false, None) }.map_err(|e| e.to_string())?,
        );
        let stop_handle = stop.0;
        let state = Arc::new(AtomicU8::new(WAITING));
        let worker_state = state.clone();
        let (arm, commands) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("DXGI frame readiness".into())
            .spawn(move || {
                while commands.recv().is_ok() {
                    let result =
                        unsafe { WaitForMultipleObjects(&[stop_handle, ready], false, INFINITE) };
                    if result == WAIT_OBJECT_0 {
                        break;
                    }
                    let value = if result.0 == WAIT_OBJECT_0.0 + 1 {
                        READY
                    } else {
                        FAILED
                    };
                    worker_state.store(value, Ordering::Release);
                    if !notify() || value == FAILED {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let waiter = Self {
            state,
            arm: Some(arm),
            worker: Some(worker),
            stop,
        };
        waiter.arm()?;
        Ok(waiter)
    }

    pub fn is_ready(&self) -> Result<bool, String> {
        match self.state.load(Ordering::Acquire) {
            READY => Ok(true),
            FAILED => Err("DXGI frame event failed".into()),
            _ => Ok(false),
        }
    }

    /// Called only after a successful Present consumed the previous slot.
    pub fn arm(&self) -> Result<(), String> {
        self.state.store(WAITING, Ordering::Release);
        self.arm
            .as_ref()
            .unwrap()
            .try_send(())
            .map_err(|e| e.to_string())
    }
}

impl Drop for FrameWaiter {
    fn drop(&mut self) {
        unsafe {
            let _ = SetEvent(self.stop.0);
        }
        self.arm.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn readiness_is_delivered_once_per_arm_and_shutdown_interrupts_waits() {
        let ready = OwnedHandle(unsafe { CreateEventW(None, true, false, None) }.unwrap());
        let (send, receive) = mpsc::channel();
        let waiter = FrameWaiter::new(ready.0, move || send.send(()).is_ok()).unwrap();
        assert!(!waiter.is_ready().unwrap());
        unsafe {
            SetEvent(ready.0).unwrap();
        }
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(waiter.is_ready().unwrap());
        assert!(receive.recv_timeout(Duration::from_millis(20)).is_err());
        waiter.arm().unwrap();
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(waiter); // worker is waiting for the next arm

        let never_ready = OwnedHandle(unsafe { CreateEventW(None, true, false, None) }.unwrap());
        let waiter = FrameWaiter::new(never_ready.0, || true).unwrap();
        drop(waiter); // worker may be waiting inside WaitForMultipleObjects
    }
}
