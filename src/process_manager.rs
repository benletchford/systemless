//! Architecture-neutral process identity shared by guest adapters.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessSerialNumber {
    pub(crate) high: u32,
    pub(crate) low: u32,
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
}
