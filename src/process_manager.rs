//! Architecture-neutral process identity shared by guest adapters.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessSerialNumber {
    pub(crate) high: u32,
    pub(crate) low: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SingleProcessEnumeration {
    Current(ProcessSerialNumber),
    End,
    Invalid,
}

impl ProcessSerialNumber {
    pub(crate) const NONE: Self = Self::new(0, 0);
    pub(crate) const CURRENT: Self = Self::new(0, 2);

    pub(crate) const fn new(high: u32, low: u32) -> Self {
        Self { high, low }
    }

    pub(crate) const fn is_current(self) -> bool {
        self.high == Self::CURRENT.high && self.low == Self::CURRENT.low
    }

    pub(crate) const fn next_single_process(self) -> SingleProcessEnumeration {
        if self.high == Self::NONE.high && self.low == Self::NONE.low {
            SingleProcessEnumeration::Current(Self::CURRENT)
        } else if self.is_current() {
            SingleProcessEnumeration::End
        } else {
            SingleProcessEnumeration::Invalid
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_identity_is_canonical() {
        assert_eq!(ProcessSerialNumber::CURRENT, ProcessSerialNumber::new(0, 2));
        assert!(ProcessSerialNumber::CURRENT.is_current());
        assert!(!ProcessSerialNumber::NONE.is_current());
        assert!(!ProcessSerialNumber::new(0, 3).is_current());
    }

    #[test]
    fn single_process_enumeration_classifies_every_cursor_state() {
        assert_eq!(
            ProcessSerialNumber::NONE.next_single_process(),
            SingleProcessEnumeration::Current(ProcessSerialNumber::CURRENT)
        );
        assert_eq!(
            ProcessSerialNumber::CURRENT.next_single_process(),
            SingleProcessEnumeration::End
        );
        assert_eq!(
            ProcessSerialNumber::new(0, 3).next_single_process(),
            SingleProcessEnumeration::Invalid
        );
    }
}
