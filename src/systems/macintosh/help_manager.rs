//! Parsing of classic Help Manager resources.

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HelpMessage {
    String(Vec<u8>),
    Resource {
        kind: u16,
        id: i16,
        index: Option<i16>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MenuHelpMessage {
    pub options: u32,
    pub proc_id: i16,
    pub variant: i16,
    pub count: u16,
    pub message: HelpMessage,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PositionedHelpMessage {
    pub options: u32,
    pub proc_id: i16,
    pub variant: i16,
    pub count: u16,
    pub tip: [u8; 4],
    pub rect: [u8; 8],
    pub message: HelpMessage,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HelpResourceError {
    Malformed,
    WrongVersion,
    Skipped,
    Unsupported,
    OutOfMemory,
}

fn word(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn component(data: &[u8], offset: usize) -> Result<(&[u8], usize), HelpResourceError> {
    let size = usize::from(word(data, offset).ok_or(HelpResourceError::Malformed)?);
    if size < 4 {
        return Err(HelpResourceError::Malformed);
    }
    let end = offset
        .checked_add(size)
        .ok_or(HelpResourceError::Malformed)?;
    Ok((
        data.get(offset..end).ok_or(HelpResourceError::Malformed)?,
        end,
    ))
}

fn menu_component(
    data: &[u8],
    state: usize,
    states: usize,
) -> Result<Option<HelpMessage>, HelpResourceError> {
    let kind = word(data, 2).ok_or(HelpResourceError::Malformed)?;
    let payload = data.get(4..).ok_or(HelpResourceError::Malformed)?;
    menu_component_payload(kind, payload, state, states)
}

fn menu_component_payload(
    kind: u16,
    data: &[u8],
    state: usize,
    states: usize,
) -> Result<Option<HelpMessage>, HelpResourceError> {
    match kind {
        1 => {
            let mut offset = 0usize;
            let mut selected = None;
            for index in 0..states {
                let len = usize::from(*data.get(offset).ok_or(HelpResourceError::Malformed)?);
                let start = offset + 1;
                let end = start.checked_add(len).ok_or(HelpResourceError::Malformed)?;
                let text = data.get(start..end).ok_or(HelpResourceError::Malformed)?;
                if index == state {
                    if !text.is_empty() {
                        let mut copy = Vec::new();
                        copy.try_reserve_exact(text.len())
                            .map_err(|_| HelpResourceError::OutOfMemory)?;
                        copy.extend_from_slice(text);
                        selected = Some(HelpMessage::String(copy));
                    }
                }
                offset = end;
            }
            Ok(selected)
        }
        2 | 6 | 7 => {
            word(data, (states - 1) * 2).ok_or(HelpResourceError::Malformed)?;
            let id = word(data, state * 2).ok_or(HelpResourceError::Malformed)? as i16;
            Ok((id != 0).then_some(HelpMessage::Resource {
                kind,
                id,
                index: None,
            }))
        }
        3 => {
            word(data, (states - 1) * 4 + 2).ok_or(HelpResourceError::Malformed)?;
            let offset = state * 4;
            let id = word(data, offset).ok_or(HelpResourceError::Malformed)? as i16;
            let index = word(data, offset + 2).ok_or(HelpResourceError::Malformed)? as i16;
            Ok((id != 0).then_some(HelpMessage::Resource {
                kind,
                id,
                index: Some(index),
            }))
        }
        256 => Err(HelpResourceError::Skipped),
        512 => {
            // HMGetIndHelpMsg extracts the nested message without comparing
            // the name against the current menu item.
            let len = usize::from(*data.first().ok_or(HelpResourceError::Malformed)?);
            let offset = (1 + len + 1) & !1;
            let nested = data.get(offset..).ok_or(HelpResourceError::Malformed)?;
            let nested_kind = word(nested, 0).ok_or(HelpResourceError::Malformed)?;
            if nested_kind == 512 {
                return Err(HelpResourceError::Unsupported);
            }
            menu_component_payload(nested_kind, &nested[2..], state, states)
        }
        _ => Err(HelpResourceError::Unsupported),
    }
}

/// Decode a menu help resource's indexed item and state, including fallback.
pub(crate) fn menu_help_message(
    data: &[u8],
    index: u16,
    state: u16,
) -> Result<MenuHelpMessage, HelpResourceError> {
    if word(data, 0).ok_or(HelpResourceError::Malformed)? != 2 {
        return Err(HelpResourceError::WrongVersion);
    }
    let options = u32::from_be_bytes(
        data.get(2..6)
            .ok_or(HelpResourceError::Malformed)?
            .try_into()
            .unwrap(),
    );
    let proc_id = word(data, 6).ok_or(HelpResourceError::Malformed)? as i16;
    let variant = word(data, 8).ok_or(HelpResourceError::Malformed)? as i16;
    let count = word(data, 10).ok_or(HelpResourceError::Malformed)?;
    if index == 0 || index > count || state > 3 {
        return Err(HelpResourceError::Skipped);
    }
    let (missing, mut offset) = component(data, 12)?;
    for item in 1..=index {
        let (current, next) = component(data, offset)?;
        if item == index {
            let message = match menu_component(current, usize::from(state), 4)? {
                Some(message) => message,
                None => menu_component(missing, usize::from(state), 4)?
                    .ok_or(HelpResourceError::Skipped)?,
            };
            return Ok(MenuHelpMessage {
                options,
                proc_id,
                variant,
                count,
                message,
            });
        }
        offset = next;
    }
    Err(HelpResourceError::Skipped)
}

/// Decode dialog, rectangle-list, override, and Finder help resources.
pub(crate) fn positioned_help_message(
    data: &[u8],
    resource_type: [u8; 4],
    index: u16,
    state: u16,
) -> Result<PositionedHelpMessage, HelpResourceError> {
    let dialog = resource_type == *b"hdlg";
    let rectangle = resource_type == *b"hrct";
    if !dialog && !rectangle && resource_type != *b"hovr" && resource_type != *b"hfdr" {
        return Err(HelpResourceError::Unsupported);
    }
    if word(data, 0).ok_or(HelpResourceError::Malformed)? != 2 {
        return Err(HelpResourceError::WrongVersion);
    }
    let base = if dialog { 4 } else { 2 };
    let options = u32::from_be_bytes(
        data.get(base..base + 4)
            .ok_or(HelpResourceError::Malformed)?
            .try_into()
            .unwrap(),
    );
    let proc_id = word(data, base + 4).ok_or(HelpResourceError::Malformed)? as i16;
    let variant = word(data, base + 6).ok_or(HelpResourceError::Malformed)? as i16;
    let count = word(data, base + 8).ok_or(HelpResourceError::Malformed)?;
    if index == 0 || index > count || state > 3 {
        return Err(HelpResourceError::Skipped);
    }
    let mut offset = base + 10;
    let missing = if dialog || resource_type == *b"hovr" {
        let (value, next) = component(data, offset)?;
        offset = next;
        Some(value)
    } else {
        None
    };
    for item in 1..=index {
        let (current, next) = component(data, offset)?;
        if item != index {
            offset = next;
            continue;
        }
        let positioned = dialog || rectangle;
        let decode = |part: &[u8]| -> Result<Option<HelpMessage>, HelpResourceError> {
            let raw_kind = word(part, 2).ok_or(HelpResourceError::Malformed)?;
            // Balloons.r defines rectangle items with HMTrackCntlID as
            // four-state messages, even though Balloons.h reserves the flag.
            let tracked_control = rectangle && raw_kind & 0x0800 != 0;
            let kind = if tracked_control {
                raw_kind & !0x0800
            } else {
                raw_kind
            };
            if kind == 256 {
                if rectangle && part.len() < 16 {
                    return Err(HelpResourceError::Malformed);
                }
                return Err(HelpResourceError::Skipped);
            }
            let payload = if positioned { 16 } else { 4 };
            let payload = part.get(payload..).ok_or(HelpResourceError::Malformed)?;
            menu_component_payload(
                kind,
                payload,
                if dialog || tracked_control {
                    usize::from(state)
                } else {
                    0
                },
                if dialog || tracked_control { 4 } else { 1 },
            )
        };
        let message = match decode(current)? {
            Some(message) => message,
            None => match missing {
                Some(missing) => decode(missing)?.ok_or(HelpResourceError::Skipped)?,
                None => return Err(HelpResourceError::Skipped),
            },
        };
        let (tip, rect) = if positioned {
            (
                current
                    .get(4..8)
                    .ok_or(HelpResourceError::Malformed)?
                    .try_into()
                    .unwrap(),
                current
                    .get(8..16)
                    .ok_or(HelpResourceError::Malformed)?
                    .try_into()
                    .unwrap(),
            )
        } else {
            ([0; 4], [0; 8])
        };
        return Ok(PositionedHelpMessage {
            options,
            proc_id,
            variant,
            count,
            tip,
            rect,
            message,
        });
    }
    Err(HelpResourceError::Skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_help_extracts_states_and_missing_fallback() {
        let data = [
            0, 2, 0, 0, 0, 5, 0, 126, 0, 3, 0, 1, 0, 12, 0, 1, 1, b'M', 1, b'N', 0, 0, 0, 0, 0, 12,
            0, 1, 1, b'A', 0, 1, b'C', 0, 0, 0,
        ];
        let enabled = menu_help_message(&data, 1, 0).unwrap();
        assert_eq!(
            enabled,
            MenuHelpMessage {
                options: 5,
                proc_id: 126,
                variant: 3,
                count: 1,
                message: HelpMessage::String(b"A".to_vec())
            }
        );
        assert_eq!(
            menu_help_message(&data, 1, 1).unwrap().message,
            HelpMessage::String(b"N".to_vec())
        );
        assert_eq!(
            menu_help_message(&data, 2, 0),
            Err(HelpResourceError::Skipped)
        );
        assert_eq!(
            menu_help_message(&data[..20], 1, 0),
            Err(HelpResourceError::Malformed)
        );
    }

    #[test]
    fn menu_help_count_excludes_missing_items_component() {
        // Balloons.r counts HMenuArray, which follows the missing component.
        let data = [
            0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 4, 1, 0, 0, 10, 0, 1, 1, b'A', 0, 0, 0, 0, 0,
            10, 0, 1, 1, b'B', 0, 0, 0, 0,
        ];
        let title = menu_help_message(&data, 1, 0).unwrap();
        assert_eq!(title.count, 2);
        assert_eq!(title.message, HelpMessage::String(b"A".to_vec()));
        let item = menu_help_message(&data, 2, 0).unwrap();
        assert_eq!(item.count, 2);
        assert_eq!(item.message, HelpMessage::String(b"B".to_vec()));
        assert_eq!(
            menu_help_message(&data, 3, 0),
            Err(HelpResourceError::Skipped)
        );
    }

    #[test]
    fn menu_help_extracts_compare_message_without_matching_name() {
        let data = [
            0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 4, 1, 0, 0, 14, 2, 0, 1, b'X', 0, 1, 1, b'A', 1,
            b'B', 0, 0,
        ];
        assert_eq!(
            menu_help_message(&data, 1, 1).unwrap().message,
            HelpMessage::String(b"B".to_vec())
        );
    }

    #[test]
    fn menu_help_rejects_truncated_unselected_states() {
        let data = [
            0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 4, 1, 0, 0, 6, 0, 1, 1, b'A',
        ];
        assert_eq!(
            menu_help_message(&data, 1, 0),
            Err(HelpResourceError::Malformed)
        );
    }

    #[test]
    fn menu_help_selects_string_list_and_resource_fallback() {
        let data = [
            0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 12, 0, 7, 0, 10, 0, 11, 0, 12, 0, 13, 0, 20, 0,
            3, 0, 20, 0, 1, 0, 0, 0, 0, 0, 22, 0, 3, 0, 23, 0, 4,
        ];
        assert_eq!(
            menu_help_message(&data, 1, 0).unwrap().message,
            HelpMessage::Resource {
                kind: 3,
                id: 20,
                index: Some(1)
            }
        );
        assert_eq!(
            menu_help_message(&data, 1, 1).unwrap().message,
            HelpMessage::Resource {
                kind: 7,
                id: 11,
                index: None
            }
        );
        assert_eq!(
            menu_help_message(&data, 1, 3).unwrap().message,
            HelpMessage::Resource {
                kind: 3,
                id: 23,
                index: Some(4)
            }
        );
    }

    #[test]
    fn rectangle_help_extracts_geometry_and_message() {
        let data = [
            0, 2, 0, 0, 0, 5, 0, 126, 0, 3, 0, 1, 0, 18, 0, 1, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6,
            1, b'R',
        ];
        let result = positioned_help_message(&data, *b"hrct", 1, 0).unwrap();
        assert_eq!(result.tip, [0, 1, 0, 2]);
        assert_eq!(result.rect, [0, 3, 0, 4, 0, 5, 0, 6]);
        assert_eq!(result.message, HelpMessage::String(b"R".to_vec()));
        assert_eq!(result.count, 1);
    }

    #[test]
    fn rectangle_skip_requires_complete_geometry() {
        let header = [0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let mut short = header.to_vec();
        short.extend_from_slice(&[0, 4, 1, 0]);
        assert_eq!(
            positioned_help_message(&short, *b"hrct", 1, 0),
            Err(HelpResourceError::Malformed)
        );
        let mut complete = header.to_vec();
        complete.extend_from_slice(&[0, 16, 1, 0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6]);
        assert_eq!(
            positioned_help_message(&complete, *b"hrct", 1, 0),
            Err(HelpResourceError::Skipped)
        );
    }

    #[test]
    fn rectangle_help_extracts_tracked_control_messages() {
        let data = [
            0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 24, 8, 1, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 1,
            b'A', 1, b'B', 1, b'C', 1, b'D',
        ];
        assert_eq!(
            positioned_help_message(&data, *b"hrct", 1, 0)
                .unwrap()
                .message,
            HelpMessage::String(b"A".to_vec())
        );
        assert_eq!(
            positioned_help_message(&data, *b"hrct", 1, 2)
                .unwrap()
                .message,
            HelpMessage::String(b"C".to_vec())
        );
    }

    #[test]
    fn dialog_help_uses_missing_state_with_item_geometry() {
        let data = [
            0, 2, 0, 0, 0, 0, 0, 5, 0, 126, 0, 3, 0, 1, 0, 22, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 1, b'F', 0, 0, 0, 0, 22, 0, 1, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 1, b'A', 0,
            0, 0, 0,
        ];
        let result = positioned_help_message(&data, *b"hdlg", 1, 1).unwrap();
        assert_eq!(result.tip, [0, 1, 0, 2]);
        assert_eq!(result.message, HelpMessage::String(b"F".to_vec()));
    }

    #[test]
    fn finder_help_extracts_single_resource_reference() {
        let data = [0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 8, 0, 3, 0, 42, 0, 7];
        assert_eq!(
            positioned_help_message(&data, *b"hfdr", 1, 0)
                .unwrap()
                .message,
            HelpMessage::Resource {
                kind: 3,
                id: 42,
                index: Some(7)
            }
        );
    }

    #[test]
    fn override_help_uses_missing_message_for_zero_resource_id() {
        let data = [
            0, 2, 0, 0, 0, 1, 0, 126, 0, 0, 0, 1, 0, 6, 0, 7, 0, 42, 0, 6, 0, 7, 0, 0,
        ];
        let result = positioned_help_message(&data, *b"hovr", 1, 0).unwrap();
        assert_eq!(
            result.message,
            HelpMessage::Resource {
                kind: 7,
                id: 42,
                index: None
            }
        );
        assert_eq!(result.tip, [0; 4]);
        assert_eq!(result.rect, [0; 8]);
    }
}
