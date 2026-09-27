//! Runner audio integration and test capture utilities.

use crate::audio::AudioBackend;
use std::cell::RefCell;
use std::rc::Rc;

/// Audio backend that captures queued stereo samples into an in-memory buffer for testing.
#[derive(Clone, Default)]
pub struct CapturingAudioBackend {
    stereo_samples: Rc<RefCell<Vec<u8>>>,
}

impl CapturingAudioBackend {
    pub fn new() -> (Self, Rc<RefCell<Vec<u8>>>) {
        let stereo_samples = Rc::new(RefCell::new(Vec::new()));
        (
            Self {
                stereo_samples: stereo_samples.clone(),
            },
            stereo_samples,
        )
    }
}

impl AudioBackend for CapturingAudioBackend {
    fn queue_samples(&mut self, samples: &[u8]) {
        self.stereo_samples.borrow_mut().extend(samples);
    }

    fn queue_stereo_samples(&mut self, samples: &[u8]) {
        self.stereo_samples.borrow_mut().extend(samples);
    }

    fn stop(&mut self) {}
}
