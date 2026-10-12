//! Coalesced worker snapshots with publication-driven renderer wakeups.
use std::{
    future::Future,
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

pub(crate) struct Latest<T> {
    state: Mutex<State<T>>,
    #[cfg(any(test, feature = "gpui-demo-test"))]
    ready: std::sync::Condvar,
}

struct State<T> {
    value: Option<T>,
    waiter: Option<(u64, Waker)>,
    next_waiter: u64,
    closed: bool,
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self {
            #[cfg(any(test, feature = "gpui-demo-test"))]
            ready: std::sync::Condvar::new(),
            state: Mutex::new(State {
                value: None,
                waiter: None,
                next_waiter: 0,
                closed: false,
            }),
        }
    }
}

impl<T> Latest<T> {
    /// Replace an unconsumed snapshot instead of queuing obsolete frames.
    /// Wake and destroy displaced payloads after releasing the slot lock.
    pub(crate) fn publish(&self, value: T) -> bool {
        let (previous, waiter) = {
            let mut state = self.state.lock().unwrap();
            if state.closed {
                drop(state);
                return false;
            }
            let previous = state.value.replace(value);
            (previous, state.waiter.take())
        };
        #[cfg(any(test, feature = "gpui-demo-test"))]
        self.ready.notify_all();
        if let Some((_, waiter)) = waiter {
            waiter.wake();
        }
        drop(previous);
        true
    }

    /// Offscreen drivers consume the same publication slot synchronously.
    pub(crate) fn take(&self) -> Option<T> {
        self.state.lock().unwrap().value.take()
    }

    /// Capture drivers wait on the same slot without a periodic sleep. The
    /// predicate and publication use one mutex, preventing lost notifications.
    #[cfg(any(test, feature = "gpui-demo-test"))]
    pub(crate) fn take_timeout(&self, timeout: std::time::Duration) -> Option<T> {
        let state = self.state.lock().unwrap();
        let (mut state, _) = self.ready.wait_timeout_while(state, timeout,
            |state| state.value.is_none() && !state.closed).unwrap();
        state.value.take()
    }

    /// One renderer awaits the next snapshot; no periodic timer is required.
    pub(crate) fn next(&self) -> Next<'_, T> {
        let mut state = self.state.lock().unwrap();
        state.next_waiter = state
            .next_waiter
            .checked_add(1)
            .expect("snapshot wait identity exhausted");
        Next {
            mailbox: self,
            id: state.next_waiter,
        }
    }

    /// Closing follows the final publication, including worker error status.
    pub(crate) fn publisher(&self) -> Publisher<'_, T> {
        Publisher(self)
    }

    fn close(&self) {
        let waiter = {
            let mut state = self.state.lock().unwrap();
            state.closed = true;
            state.waiter.take()
        };
        #[cfg(any(test, feature = "gpui-demo-test"))]
        self.ready.notify_all();
        if let Some((_, waiter)) = waiter {
            waiter.wake();
        }
    }
}

pub(crate) struct Publisher<'a, T>(&'a Latest<T>);
impl<T> Drop for Publisher<'_, T> {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub(crate) struct Next<'a, T> {
    mailbox: &'a Latest<T>,
    id: u64,
}

impl<T> Future for Next<'_, T> {
    type Output = Option<T>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let waker = cx.waker().clone();
        let previous = {
            let mut state = self.mailbox.state.lock().unwrap();
            if let Some(value) = state.value.take() {
                return Poll::Ready(Some(value));
            }
            if state.closed {
                return Poll::Ready(None);
            }
            // Registration and the empty-slot check share the producer lock,
            // so a publication cannot fall between them and lose its wakeup.
            state.waiter.replace((self.id, waker))
        };
        drop(previous);
        Poll::Pending
    }
}

impl<T> Drop for Next<'_, T> {
    fn drop(&mut self) {
        let removed = {
            let mut state = self.mailbox.state.lock().unwrap();
            if state.waiter.as_ref().is_some_and(|(id, _)| *id == self.id) {
                state.waiter.take()
            } else {
                None
            }
        };
        // A cancelled old wait must not remove a replacement renderer's wake.
        drop(removed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::task::Wake;

    #[derive(Default)]
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn synchronous_capture_wait_handles_publication_races_and_close() {
        for _ in 0..100 {
            let mailbox = Arc::new(Latest::default());
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let reader = mailbox.clone(); let start = barrier.clone();
            let consumer = std::thread::spawn(move || {
                start.wait(); reader.take_timeout(std::time::Duration::from_secs(2))
            });
            barrier.wait(); assert!(mailbox.publish(42));
            assert_eq!(consumer.join().unwrap(), Some(42));
            assert_eq!(mailbox.take_timeout(std::time::Duration::ZERO), None);
            mailbox.publish(99); drop(mailbox.publisher());
            assert_eq!(mailbox.take_timeout(std::time::Duration::from_secs(2)), Some(99));
            assert_eq!(mailbox.take_timeout(std::time::Duration::from_secs(2)), None);
        }
    }

    #[test]
    fn publication_wakes_a_waiting_renderer_without_a_timer() {
        let mailbox = Latest::default();
        let count = Arc::new(Count::default());
        let waker = Waker::from(count.clone());
        let mut cx = Context::from_waker(&waker);
        let mut next = mailbox.next();
        assert_eq!(Pin::new(&mut next).poll(&mut cx), Poll::Pending);
        assert!(mailbox.publish(7));
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert_eq!(Pin::new(&mut next).poll(&mut cx), Poll::Ready(Some(7)));
    }

    #[test]
    fn worker_thread_publication_wakes_the_waiting_consumer() {
        let mailbox = Arc::new(Latest::default());
        let count = Arc::new(Count::default());
        let waker = Waker::from(count.clone());
        let mut next = mailbox.next();
        assert_eq!(
            Pin::new(&mut next).poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        let output = mailbox.clone();
        std::thread::spawn(move || assert!(output.publish(42)))
            .join()
            .unwrap();
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            Pin::new(&mut next).poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Some(42))
        );
    }

    #[test]
    fn slow_renderers_receive_the_latest_snapshot_without_a_backlog() {
        let mailbox = Latest::default();
        mailbox.publish(1);
        mailbox.publish(2);
        let waker = Waker::from(Arc::new(Count::default()));
        let mut cx = Context::from_waker(&waker);
        assert_eq!(
            Pin::new(&mut mailbox.next()).poll(&mut cx),
            Poll::Ready(Some(2))
        );
        assert_eq!(mailbox.take(), None);
    }

    #[test]
    fn cancelling_an_old_wait_preserves_the_replacement_waker() {
        let mailbox = Latest::default();
        let first = Arc::new(Count::default());
        let second = Arc::new(Count::default());
        let first_waker = Waker::from(first.clone());
        let second_waker = Waker::from(second.clone());
        let mut old = mailbox.next();
        let mut replacement = mailbox.next();
        assert_eq!(
            Pin::new(&mut old).poll(&mut Context::from_waker(&first_waker)),
            Poll::Pending
        );
        assert_eq!(
            Pin::new(&mut replacement).poll(&mut Context::from_waker(&second_waker)),
            Poll::Pending
        );
        drop(old);
        mailbox.publish(3);
        assert_eq!(first.0.load(Ordering::SeqCst), 0);
        assert_eq!(second.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            Pin::new(&mut replacement).poll(&mut Context::from_waker(&second_waker)),
            Poll::Ready(Some(3))
        );
    }

    #[test]
    fn cancelled_waits_release_their_registration() {
        let mailbox = Latest::default();
        let count = Arc::new(Count::default());
        let waker = Waker::from(count.clone());
        let mut next = mailbox.next();
        assert_eq!(
            Pin::new(&mut next).poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        drop(next);
        mailbox.publish(4);
        assert_eq!(count.0.load(Ordering::SeqCst), 0);
        assert_eq!(Arc::strong_count(&count), 2);
    }

    #[test]
    fn worker_exit_delivers_its_final_snapshot_before_exhaustion() {
        let mailbox = Latest::default();
        let publisher = mailbox.publisher();
        mailbox.publish(99);
        drop(publisher);
        let waker = Waker::from(Arc::new(Count::default()));
        let mut cx = Context::from_waker(&waker);
        assert_eq!(
            Pin::new(&mut mailbox.next()).poll(&mut cx),
            Poll::Ready(Some(99))
        );
        assert_eq!(
            Pin::new(&mut mailbox.next()).poll(&mut cx),
            Poll::Ready(None)
        );
        assert!(!mailbox.publish(100));
    }

    #[test]
    fn worker_exit_wakes_an_empty_pending_renderer() {
        let mailbox = Latest::<usize>::default();
        let count = Arc::new(Count::default());
        let waker = Waker::from(count.clone());
        let mut next = mailbox.next();
        assert_eq!(
            Pin::new(&mut next).poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        drop(mailbox.publisher());
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            Pin::new(&mut next).poll(&mut Context::from_waker(&waker)),
            Poll::Ready(None)
        );
    }

    #[test]
    fn wake_callbacks_run_outside_the_slot_lock() {
        struct Reentrant(Arc<Latest<usize>>);
        impl Wake for Reentrant {
            fn wake(self: Arc<Self>) {
                assert!(self.0.state.try_lock().is_ok());
            }
        }
        let mailbox = Arc::new(Latest::default());
        let waker = Waker::from(Arc::new(Reentrant(mailbox.clone())));
        let mut next = mailbox.next();
        assert_eq!(
            Pin::new(&mut next).poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        mailbox.publish(1);
        assert_eq!(
            Pin::new(&mut next).poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Some(1))
        );
        let mut next = mailbox.next();
        assert_eq!(
            Pin::new(&mut next).poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        drop(mailbox.publisher());
    }

    #[test]
    fn displaced_and_rejected_payloads_drop_outside_the_slot_lock() {
        struct Payload {
            mailbox: std::sync::Weak<Latest<Payload>>,
            dropped: Arc<AtomicUsize>,
        }
        impl Drop for Payload {
            fn drop(&mut self) {
                if let Some(mailbox) = self.mailbox.upgrade() {
                    assert!(mailbox.state.try_lock().is_ok());
                    self.dropped.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        let mailbox = Arc::new(Latest::default());
        let dropped = Arc::new(AtomicUsize::new(0));
        let payload = || Payload {
            mailbox: Arc::downgrade(&mailbox),
            dropped: dropped.clone(),
        };
        mailbox.publish(payload());
        mailbox.publish(payload());
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        drop(mailbox.take());
        assert_eq!(dropped.load(Ordering::SeqCst), 2);
        drop(mailbox.publisher());
        assert!(!mailbox.publish(payload()));
        assert_eq!(dropped.load(Ordering::SeqCst), 3);
    }
}
