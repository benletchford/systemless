//! Text-only host clipboard policy. Private application scrap stays guest-owned.

use systemless::systems::macintosh::mac_roman::encode_mac_roman_char;

#[derive(Default)]
pub(crate) struct HostClipboard {
    observed: Option<Option<String>>,
}

impl HostClipboard {
    /// Compare host contents, not guest scrap: an unchanged host clipboard must
    /// never replace a newer guest copy. Unsupported text and non-text contents
    /// are remembered but leave all guest flavors intact.
    pub(crate) fn changed_text(&mut self, text: Option<String>) -> Option<Vec<u8>> {
        if self.observed.as_ref() == Some(&text) {
            return None;
        }
        self.observed = Some(text.clone());
        let text = text?;
        encode_text(&text)
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
}
