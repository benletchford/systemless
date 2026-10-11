//! Architecture-neutral classic Macintosh Operating System Queue Utilities evaluation.
//!
//! Provides canonical evaluation for OS queue management operations,
//! primarily `Enqueue` (trap `$A96F` / InterfaceLib `Enqueue`) and `Dequeue` (trap `$A96E` / InterfaceLib `Dequeue`).
//!
//! References:
//! - Inside Macintosh Volume II (1985), pp. II-382 to II-384
//! - Inside Macintosh: Operating System Utilities (1994), pp. 6-3 to 6-18

#![allow(dead_code)]

/// Trap word for Enqueue (`$A96F`).
pub const ENQUEUE_TRAP: u16 = 0xA96F;

/// Toolbox trap offset for Enqueue (`$A96F & 0x03FF = 0x016F`).
pub const ENQUEUE_TRAP_OFFSET: u16 = 0x016F;

/// Trap word for Dequeue (`$A96E`).
pub const DEQUEUE_TRAP: u16 = 0xA96E;

/// Toolbox trap offset for Dequeue (`$A96E & 0x03FF = 0x016E`).
pub const DEQUEUE_TRAP_OFFSET: u16 = 0x016E;

/// Offset of `qFlags` in `QHdr` (Integer, 2 bytes).
pub const Q_FLAGS_OFFSET: u32 = 0;

/// Offset of `qHead` in `QHdr` (QElemPtr, 4 bytes).
pub const Q_HEAD_OFFSET: u32 = 2;

/// Offset of `qTail` in `QHdr` (QElemPtr, 4 bytes).
pub const Q_TAIL_OFFSET: u32 = 6;

/// Total size in bytes of a standard `QHdr` (10 bytes).
pub const Q_HDR_SIZE: u32 = 10;

/// Offset of `qLink` in `QElem` (QElemPtr, 4 bytes).
pub const Q_LINK_OFFSET: u32 = 0;

/// Offset of `qType` in `QElem` (Integer / QTypes, 2 bytes).
pub const Q_TYPE_OFFSET: u32 = 4;

/// Total size in bytes of a standard `QElem` header (6 bytes).
pub const Q_ELEM_HEADER_SIZE: u32 = 6;

/// Successful completion code (`noErr = 0`).
pub const NO_ERR: i16 = 0;

/// Element not found in queue (`qErr = -1`).
pub const Q_ERR: i16 = -1;

/// Represents the header of an operating system queue (`QHdr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueHeader {
    pub flags: u16,
    pub head: u32,
    pub tail: u32,
}

impl QueueHeader {
    /// Returns true if the queue contains no elements (`qHead == 0`).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head == 0
    }

    /// Deserializes a `QHdr` from 10 big-endian bytes.
    pub fn from_bytes(bytes: &[u8; 10]) -> Self {
        let flags = u16::from_be_bytes([bytes[0], bytes[1]]);
        let head = u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        let tail = u32::from_be_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
        Self { flags, head, tail }
    }

    /// Serializes a `QHdr` to 10 big-endian bytes.
    pub fn to_bytes(&self) -> [u8; 10] {
        let mut bytes = [0u8; 10];
        bytes[0..2].copy_from_slice(&self.flags.to_be_bytes());
        bytes[2..6].copy_from_slice(&self.head.to_be_bytes());
        bytes[6..10].copy_from_slice(&self.tail.to_be_bytes());
        bytes
    }
}

/// Result of evaluating an `Enqueue` operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueEvaluation {
    /// Memory writes to perform: pairs of (destination_address, 32_bit_be_value).
    pub writes: Vec<(u32, u32)>,
}

/// Evaluates appending an element to an OS queue (`Enqueue`).
///
/// Parameters:
/// - `element`: Pointer to the queue element (`QElemPtr`).
/// - `header`: Pointer to the queue header (`QHdrPtr`).
/// - `head`: Current value of `header.qHead`.
/// - `tail`: Current value of `header.qTail`.
/// - `read_link`: Closure to read the 32-bit `qLink` of any element address.
pub fn evaluate_enqueue(
    element: u32,
    header: u32,
    head: u32,
    tail: u32,
    mut read_link: impl FnMut(u32) -> u32,
) -> EnqueueEvaluation {
    if element == 0 || header == 0 {
        return EnqueueEvaluation { writes: Vec::new() };
    }

    let mut writes = Vec::with_capacity(3);

    // 1. Terminate the new element by clearing its qLink
    writes.push((element + Q_LINK_OFFSET, 0));

    // 2. If the queue is empty, set both head and tail to the new element
    if head == 0 {
        writes.push((header + Q_HEAD_OFFSET, element));
        writes.push((header + Q_TAIL_OFFSET, element));
        return EnqueueEvaluation { writes };
    }

    // 3. If non-empty, find the current tail element
    let current_tail = if tail != 0 {
        tail
    } else {
        // Fallback: walk the chain from head if tail was 0
        let mut cursor = head;
        let mut loop_count = 0;
        loop {
            let next = read_link(cursor);
            if next == 0 || loop_count > 100_000 {
                break cursor;
            }
            cursor = next;
            loop_count += 1;
        }
    };

    // 4. Link previous tail to new element, and update header.qTail
    writes.push((current_tail + Q_LINK_OFFSET, element));
    writes.push((header + Q_TAIL_OFFSET, element));

    EnqueueEvaluation { writes }
}

/// Result of evaluating a `Dequeue` operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DequeueEvaluation {
    /// Result code: `NO_ERR` (0) if element was found and removed, `Q_ERR` (-1) otherwise.
    pub result: i16,
    /// Memory writes to perform: pairs of (destination_address, 32_bit_be_value).
    pub writes: Vec<(u32, u32)>,
}

/// Evaluates removing an element from an OS queue (`Dequeue`).
///
/// Parameters:
/// - `element`: Pointer to the queue element to find and remove (`QElemPtr`).
/// - `header`: Pointer to the queue header (`QHdrPtr`).
/// - `head`: Current value of `header.qHead`.
/// - `tail`: Current value of `header.qTail`.
/// - `read_link`: Closure to read the 32-bit `qLink` of any element address.
pub fn evaluate_dequeue(
    element: u32,
    header: u32,
    head: u32,
    tail: u32,
    mut read_link: impl FnMut(u32) -> u32,
) -> DequeueEvaluation {
    if element == 0 || header == 0 || head == 0 {
        return DequeueEvaluation {
            result: Q_ERR,
            writes: Vec::new(),
        };
    }

    let mut prev = 0;
    let mut current = head;
    let mut loop_count = 0;

    while current != 0 && loop_count <= 100_000 {
        let next = read_link(current);
        if current == element {
            let mut writes = Vec::with_capacity(3);

            if prev == 0 {
                // Removing the head of the queue
                writes.push((header + Q_HEAD_OFFSET, next));
            } else {
                // Removing a middle or tail element
                writes.push((prev + Q_LINK_OFFSET, next));
            }

            if tail == current {
                // Removing the tail element: new tail is the previous element
                writes.push((header + Q_TAIL_OFFSET, prev));
            }

            return DequeueEvaluation {
                result: NO_ERR,
                writes,
            };
        }

        prev = current;
        current = next;
        loop_count += 1;
    }

    DequeueEvaluation {
        result: Q_ERR,
        writes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_trap_constants() {
        assert_eq!(ENQUEUE_TRAP, 0xA96F);
        assert_eq!(ENQUEUE_TRAP_OFFSET, 0x016F);
        assert_eq!(DEQUEUE_TRAP, 0xA96E);
        assert_eq!(DEQUEUE_TRAP_OFFSET, 0x016E);
        assert_eq!(0xA800 | ENQUEUE_TRAP_OFFSET, ENQUEUE_TRAP);
        assert_eq!(0xA800 | DEQUEUE_TRAP_OFFSET, DEQUEUE_TRAP);
    }

    #[test]
    fn test_queue_header_round_trip() {
        let qhdr = QueueHeader {
            flags: 0x1234,
            head: 0x0010_0000,
            tail: 0x0020_0000,
        };
        let bytes = qhdr.to_bytes();
        let decoded = QueueHeader::from_bytes(&bytes);
        assert_eq!(decoded, qhdr);
        assert!(!decoded.is_empty());

        let empty = QueueHeader::default();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_evaluate_enqueue_empty_queue() {
        let eval = evaluate_enqueue(0x2000, 0x1000, 0, 0, |_| 0);
        assert_eq!(
            eval.writes,
            vec![
                (0x2000 + Q_LINK_OFFSET, 0),
                (0x1000 + Q_HEAD_OFFSET, 0x2000),
                (0x1000 + Q_TAIL_OFFSET, 0x2000),
            ]
        );
    }

    #[test]
    fn test_evaluate_enqueue_non_empty_queue() {
        let eval = evaluate_enqueue(0x3000, 0x1000, 0x2000, 0x2000, |_| 0);
        assert_eq!(
            eval.writes,
            vec![
                (0x3000 + Q_LINK_OFFSET, 0),
                (0x2000 + Q_LINK_OFFSET, 0x3000),
                (0x1000 + Q_TAIL_OFFSET, 0x3000),
            ]
        );
    }

    #[test]
    fn test_evaluate_dequeue_head_element() {
        let links: HashMap<u32, u32> = HashMap::from([(0x2000, 0x3000), (0x3000, 0)]);
        let eval = evaluate_dequeue(0x2000, 0x1000, 0x2000, 0x3000, |ptr| {
            links.get(&ptr).copied().unwrap_or(0)
        });
        assert_eq!(eval.result, NO_ERR);
        assert_eq!(eval.writes, vec![(0x1000 + Q_HEAD_OFFSET, 0x3000)]);
    }

    #[test]
    fn test_evaluate_dequeue_tail_element() {
        let links: HashMap<u32, u32> = HashMap::from([(0x2000, 0x3000), (0x3000, 0)]);
        let eval = evaluate_dequeue(0x3000, 0x1000, 0x2000, 0x3000, |ptr| {
            links.get(&ptr).copied().unwrap_or(0)
        });
        assert_eq!(eval.result, NO_ERR);
        assert_eq!(
            eval.writes,
            vec![
                (0x2000 + Q_LINK_OFFSET, 0),
                (0x1000 + Q_TAIL_OFFSET, 0x2000),
            ]
        );
    }

    #[test]
    fn test_evaluate_dequeue_sole_element() {
        let links: HashMap<u32, u32> = HashMap::from([(0x2000, 0)]);
        let eval = evaluate_dequeue(0x2000, 0x1000, 0x2000, 0x2000, |ptr| {
            links.get(&ptr).copied().unwrap_or(0)
        });
        assert_eq!(eval.result, NO_ERR);
        assert_eq!(
            eval.writes,
            vec![
                (0x1000 + Q_HEAD_OFFSET, 0),
                (0x1000 + Q_TAIL_OFFSET, 0),
            ]
        );
    }

    #[test]
    fn test_evaluate_dequeue_missing_element() {
        let links: HashMap<u32, u32> = HashMap::from([(0x2000, 0x3000), (0x3000, 0)]);
        let eval = evaluate_dequeue(0x9999, 0x1000, 0x2000, 0x3000, |ptr| {
            links.get(&ptr).copied().unwrap_or(0)
        });
        assert_eq!(eval.result, Q_ERR);
        assert!(eval.writes.is_empty());
    }

    #[test]
    fn test_evaluate_null_parameters() {
        let eval_enq = evaluate_enqueue(0, 0x1000, 0, 0, |_| 0);
        assert!(eval_enq.writes.is_empty());

        let eval_deq = evaluate_dequeue(0, 0x1000, 0x2000, 0x2000, |_| 0);
        assert_eq!(eval_deq.result, Q_ERR);
        assert!(eval_deq.writes.is_empty());
    }
}
