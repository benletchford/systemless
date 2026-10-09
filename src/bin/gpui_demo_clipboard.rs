//! Text-only host clipboard policy. Private application scrap stays guest-owned.

use systemless::systems::macintosh::{
    mac_roman::{decode_mac_roman, encode_mac_roman_char},
    session::MacintoshSession,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostSample {
    pub text: Option<String>,
    // Unknown/mixed formats cannot be compared faithfully through this bridge.
    pub text_only: bool,
    pub revision: Option<isize>,
}

#[derive(Default)]
pub(crate) struct HostClipboard {
    observed: Option<HostSample>,
    suspended: Option<(u64, HostSample)>,
}

impl HostClipboard {
    pub(crate) fn suspend(&mut self, generation: u64, sample: HostSample) {
        self.suspended = Some((generation, sample));
    }

    pub(crate) fn resume(&mut self) {
        self.suspended = None;
    }

    /// Ignore old cycles and host copies made while guest suspend was pending.
    /// Non-text/mixed formats are deliberately never replaced by plain text.
    pub(crate) fn export(
        &mut self,
        generation: u64,
        bytes: &[u8],
        current: HostSample,
    ) -> Option<String> {
        let (pending, baseline) = self.suspended.as_ref()?;
        if *pending != generation {
            return None;
        }
        let allowed = baseline.text_only && current.text_only && *baseline == current;
        self.suspended = None;
        if !allowed {
            return None;
        }
        let text = decode_mac_roman(bytes).replace('\r', "\n");
        self.observed = Some(HostSample {
            text: Some(text.clone()),
            text_only: true,
            revision: current.revision,
        });
        Some(text)
    }

    /// Compare host contents, not guest scrap: an unchanged host clipboard must
    /// never replace a newer guest copy. Unsupported text and non-text contents
    /// are remembered but leave all guest flavors intact.
    #[cfg(test)]
    pub(crate) fn changed_text(&mut self, text: Option<String>) -> Option<Vec<u8>> {
        self.changed_sample(HostSample {
            text,
            text_only: true,
            revision: None,
        })
    }

    pub(crate) fn changed_sample(&mut self, sample: HostSample) -> Option<Vec<u8>> {
        if self.observed.as_ref() == Some(&sample) {
            return None;
        }
        self.observed = Some(sample.clone());
        encode_text(&sample.text?)
    }

    pub(crate) fn record_export(&mut self, actual: HostSample) {
        self.observed = Some(actual);
    }
}

/// Worker-owned export retained across dropped/coalesced presentation updates.
#[derive(Default)]
pub(crate) struct GuestClipboard {
    known: Option<Vec<u8>>,
    pending: Option<u64>,
    pub export: Option<(u64, Vec<u8>)>,
}

impl GuestClipboard {
    pub(crate) fn imported(&mut self, text: &[u8]) {
        self.known = Some(text.to_vec());
    }

    pub(crate) fn foreground(&mut self, active: bool, generation: u64) {
        self.pending = (!active).then_some(generation);
        self.export = None;
    }

    pub(crate) fn observe(&mut self, session: &MacintoshSession) {
        let Some(generation) = self.pending else {
            return;
        };
        let Some(text) = session.clipboard_text_after_suspend() else {
            return;
        };
        self.pending = None;
        if text != self.known {
            self.export = text.as_ref().map(|text| (generation, text.clone()));
            self.known = text;
        }
    }
}

fn encode_text(text: &str) -> Option<Vec<u8>> {
    // A CRLF is one Macintosh line break; bare LF and CR are also accepted.
    // Reject the whole value if any character is unrepresentable rather than
    // silently corrupting copied text with replacement characters.
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    normalized.chars().map(encode_mac_roman_char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_text_is_exact_and_uses_guest_line_endings() {
        assert_eq!(
            encode_text("café\r\nnext\nlast\r"),
            Some(b"caf\x8e\rnext\rlast\r".to_vec())
        );
        assert_eq!(encode_text(""), Some(Vec::new()));
        assert_eq!(encode_text("valid prefix 🦀"), None);
    }

    #[test]
    fn unchanged_host_text_preserves_newer_guest_scrap() {
        let mut clipboard = HostClipboard::default();
        assert_eq!(
            clipboard.changed_text(Some("first".into())),
            Some(b"first".to_vec())
        );
        assert_eq!(clipboard.changed_text(Some("first".into())), None);
        assert_eq!(clipboard.changed_text(None), None);
        assert_eq!(clipboard.changed_text(Some("🦀".into())), None);
        assert_eq!(
            clipboard.changed_text(Some("first".into())),
            Some(b"first".to_vec())
        );
    }

    #[test]
    fn export_rejects_stale_cycles_new_host_copies_and_unknown_formats() {
        let sample = |text: &str| HostSample {
            text: Some(text.into()),
            text_only: true,
            revision: None,
        };
        let mut clipboard = HostClipboard::default();
        clipboard.suspend(1, sample("old"));
        assert_eq!(clipboard.export(0, b"copy", sample("old")), None);
        assert_eq!(clipboard.export(1, b"copy", sample("new host copy")), None);
        clipboard.suspend(
            2,
            HostSample {
                text: None,
                text_only: false,
                revision: None,
            },
        );
        assert_eq!(
            clipboard.export(
                2,
                b"copy",
                HostSample {
                    text: None,
                    text_only: false,
                    revision: None,
                }
            ),
            None
        );
        clipboard.suspend(3, sample("old"));
        assert_eq!(
            clipboard.export(3, b"caf\x8e\rnext", sample("old")),
            Some("café\nnext".into())
        );
        assert_eq!(clipboard.export(3, b"copy", sample("old")), None);
        assert_eq!(
            clipboard.changed_text(Some("café\nnext".into())),
            None,
            "export must not echo as a changed import"
        );
        clipboard.suspend(4, sample("old"));
        clipboard.resume();
        assert_eq!(clipboard.export(4, b"late", sample("old")), None);
    }

    #[test]
    fn native_revision_preserves_identical_new_host_copies_and_prevents_echo() {
        let sample = |revision| HostSample {
            text: Some("same text".into()),
            text_only: true,
            revision: Some(revision),
        };
        let mut clipboard = HostClipboard::default();
        clipboard.suspend(1, sample(10));
        assert_eq!(clipboard.export(1, b"guest", sample(11)), None);
        clipboard.suspend(2, sample(11));
        assert_eq!(
            clipboard.export(2, b"guest", sample(11)),
            Some("guest".into())
        );
        let written = HostSample {
            text: Some("guest".into()),
            text_only: true,
            revision: Some(12),
        };
        clipboard.record_export(written.clone());
        assert_eq!(clipboard.changed_sample(written), None);
    }
}
