//! Best-effort MacsBug symbol recovery for 68K guest code.
//!
//! Classic 68K compilers could append a procedure name after a routine's final
//! return instruction. The names are optional debugging output, not part of the
//! ABI: a binary built without them, or stripped of them, yields nothing here.
//! PowerPC code uses an unrelated convention and is not covered.
//!
//! The symbol therefore *trails* the routine it names. To answer "which routine
//! contains this address", scan forward to the nearest following symbol, not
//! backward — [`resolve_containing`] does this.
//!
//! Recovery is a pattern match over guest memory, so it is a heuristic. A
//! terminator instruction is required immediately before a candidate to keep
//! ordinary string data from matching.

use super::adapters::{read_m68k_memory, M68K_SPACE};
use super::error::DebugResult;
use super::model::DebugAddress;
use crate::runner::FixtureRunner;
use serde::{Deserialize, Serialize};

/// Instructions a routine can end with, immediately preceding a symbol.
/// RTS, JMP (A0), UNLK A6, and RTE respectively.
const TERMINATORS: [u16; 4] = [0x4E75, 0x4ED0, 0x4E5E, 0x4E74];

/// Instructions that actually transfer control out of a routine, used to detect
/// that a routine ended. Deliberately excludes `UNLK A6`, which precedes the
/// `RTS` in an ordinary epilogue rather than ending anything: treating it as a
/// routine end would misreport every normal return.
const ROUTINE_ENDS: [u16; 3] = [0x4E75, 0x4ED0, 0x4E74];

/// Variable-length format: one byte of `0x80 | length`, then `length`
/// characters. Shorter runs match too much ordinary data to be worth trusting.
const MIN_NAME_LEN: usize = 3;
const MAX_NAME_LEN: usize = 31;

/// Default window for `resolve_containing`. Large enough to clear a big
/// routine, small enough that a miss costs little.
pub const DEFAULT_RESOLVE_WINDOW: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolEntry {
    /// Address of the symbol's length byte, which is just past the end of the
    /// routine it names — not the routine's entry point.
    pub address: DebugAddress,
    pub name: String,
}

/// Approximate retained size of one entry, for the snapshot byte budget.
fn entry_bytes(entry: &SymbolEntry) -> u64 {
    (std::mem::size_of::<SymbolEntry>() + entry.name.len()) as u64
}

fn is_name_byte(byte: u8, first: bool) -> bool {
    let character = byte as char;
    if first {
        character.is_ascii_alphabetic() || character == '_'
    } else {
        character.is_ascii_alphanumeric() || character == '_' || character == '.'
    }
}

/// Decode a symbol at `offset` within `data`, if one is present there.
/// Requires a terminator instruction in the two bytes before it.
fn symbol_at(data: &[u8], offset: usize) -> Option<String> {
    // The candidate needs the two-byte terminator before it and the length
    // byte at `offset`; either read can fall outside the buffer, and callers
    // legitimately probe one past the end of a truncated window.
    if offset < 2 || offset >= data.len() {
        return None;
    }
    let previous = u16::from_be_bytes([data[offset - 2], data[offset - 1]]);
    if !TERMINATORS.contains(&previous) {
        return None;
    }
    let length = usize::from(data[offset] & 0x1f);
    if data[offset] & 0x80 == 0 || !(MIN_NAME_LEN..=MAX_NAME_LEN).contains(&length) {
        return None;
    }
    let name = data.get(offset + 1..offset + 1 + length)?;
    if !name
        .iter()
        .enumerate()
        .all(|(index, byte)| is_name_byte(*byte, index == 0))
    {
        return None;
    }
    // Checked ASCII above, so this cannot fail.
    Some(String::from_utf8(name.to_vec()).ok()?)
}

/// Scan `[start, start + length)` for symbols, stopping at whichever of
/// `entry_limit` or `byte_budget` is reached first. The bool reports whether a
/// limit cut the scan short.
pub fn scan(
    runner: &FixtureRunner,
    start: u64,
    length: u64,
    entry_limit: usize,
    byte_budget: u64,
) -> DebugResult<(Vec<SymbolEntry>, bool)> {
    let memory = read_m68k_memory(runner, start, length)?;
    let data = &memory.bytes;
    let mut symbols = Vec::new();
    let mut used = 0u64;
    let mut truncated = false;
    for offset in 0..data.len() {
        let Some(name) = symbol_at(data, offset) else {
            continue;
        };
        let entry = SymbolEntry {
            address: DebugAddress::new(M68K_SPACE, start + offset as u64),
            name,
        };
        let cost = entry_bytes(&entry);
        if symbols.len() >= entry_limit || used + cost > byte_budget {
            truncated = true;
            break;
        }
        used += cost;
        symbols.push(entry);
    }
    // A short read means the range left mapped memory before it was exhausted.
    Ok((symbols, truncated || memory.truncated))
}

/// Outcome of attributing an address to a routine.
///
/// A plain "nearest following symbol" answer is wrong whenever the address sits
/// in a routine that carries no symbol: the scan runs past that routine's end
/// and returns a later routine's name, with nothing to signal the error. These
/// variants keep the two cases apart.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SymbolResolution {
    /// The address lies in the routine named by `symbol`, `distance` bytes
    /// before its trailing name.
    Contained { symbol: SymbolEntry, distance: u64 },
    /// A routine ended without a trailing symbol before any symbol was reached,
    /// so the address belongs to an unsymbolised routine. `next` is the first
    /// symbol seen afterwards and is reported for orientation only — it names a
    /// later routine, not this one.
    Unsymbolised { next: Option<SymbolEntry> },
    /// No symbol at all within the window.
    NotFound,
}

/// Attribute `address` to the routine containing it by scanning forward for the
/// symbol that trails it.
///
/// The scan stops trusting its answer once it crosses a routine end that is not
/// followed by a symbol, because the address's own routine has then finished
/// unnamed. Without that check an unsymbolised routine silently inherits the
/// name of whatever routine happens to follow it.
///
/// TODO: fold in the jump table at `A5+CurJTOffset`, whose `JMP abs.L` entries
/// are authoritative routine entry points rather than a heuristic, and use the
/// loader's segment bases instead of a fixed scan range. Both need the loader to
/// retain what it currently only traces (`trap::dispatch::record_segment_base`),
/// which is why this is still pattern matching alone.
pub fn resolve_containing(
    runner: &FixtureRunner,
    address: u64,
    window: u64,
) -> DebugResult<SymbolResolution> {
    let memory = read_m68k_memory(runner, address, window)?;
    let data = &memory.bytes;
    let mut ended_unnamed = false;
    for offset in 0..data.len() {
        if let Some(name) = symbol_at(data, offset) {
            let symbol = SymbolEntry {
                address: DebugAddress::new(M68K_SPACE, address + offset as u64),
                name,
            };
            return Ok(if ended_unnamed {
                SymbolResolution::Unsymbolised {
                    next: Some(symbol),
                }
            } else {
                SymbolResolution::Contained {
                    symbol,
                    distance: offset as u64,
                }
            });
        }
        // Instructions are word-aligned on even addresses, so only consider a
        // routine end where one could actually be encoded.
        if (address + offset as u64) % 2 == 0 && offset + 2 <= data.len() {
            let word = u16::from_be_bytes([data[offset], data[offset + 1]]);
            if ROUTINE_ENDS.contains(&word) && symbol_at(data, offset + 2).is_none() {
                ended_unnamed = true;
            }
        }
    }
    Ok(if ended_unnamed {
        SymbolResolution::Unsymbolised { next: None }
    } else {
        SymbolResolution::NotFound
    })
}

/// Every symbol named `name` within `[start, start + length)`, ascending by
/// address. Matching is exact and case-sensitive.
///
/// Deliberately returns all matches rather than the first. Guest code is
/// routinely resident twice — a loaded CODE segment and the Resource Manager's
/// cached copy of the same resource — so a name maps to several addresses, only
/// one of which is executing. Returning the lowest would silently hand back a
/// non-executing copy whenever the cached one happens to load first.
pub fn lookup(
    runner: &FixtureRunner,
    start: u64,
    length: u64,
    name: &str,
) -> DebugResult<Vec<SymbolEntry>> {
    let memory = read_m68k_memory(runner, start, length)?;
    let data = &memory.bytes;
    let mut found = Vec::new();
    for offset in 0..data.len() {
        if symbol_at(data, offset).as_deref() == Some(name) {
            found.push(SymbolEntry {
                address: DebugAddress::new(M68K_SPACE, start + offset as u64),
                name: name.to_string(),
            });
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::bus::MemoryBus;
    use crate::runner::FixtureRunnerConfig;

    // `RTS` then a length byte of 0x80|10 then "GetAnEvent", the shape observed
    // in a real 68K binary.
    fn encoded(terminator: u16, name: &str) -> Vec<u8> {
        let mut bytes = terminator.to_be_bytes().to_vec();
        bytes.push(0x80 | name.len() as u8);
        bytes.extend_from_slice(name.as_bytes());
        bytes
    }

    #[test]
    fn decodes_a_symbol_after_each_terminator() {
        for terminator in TERMINATORS {
            let data = encoded(terminator, "GetAnEvent");
            assert_eq!(symbol_at(&data, 2).as_deref(), Some("GetAnEvent"));
        }
    }

    #[test]
    fn requires_a_terminator_before_the_name() {
        // Identical bytes, but preceded by an ordinary instruction: no symbol.
        let data = encoded(0x4E71, "GetAnEvent");
        assert_eq!(symbol_at(&data, 2), None);
    }

    #[test]
    fn rejects_names_that_are_not_identifiers() {
        for name in ["9bad", " lead", "has-dash"] {
            let data = encoded(0x4E75, name);
            assert_eq!(symbol_at(&data, 2), None, "should reject {name:?}");
        }
    }

    #[test]
    fn rejects_lengths_outside_the_accepted_range() {
        let short = encoded(0x4E75, "ab");
        assert_eq!(symbol_at(&short, 2), None);
        // High bit clear is not a length byte at all.
        let mut plain = encoded(0x4E75, "Routine");
        plain[2] &= 0x7f;
        assert_eq!(symbol_at(&plain, 2), None);
    }

    fn guest_runner() -> FixtureRunner {
        FixtureRunner::new(0x100000, FixtureRunnerConfig::default())
    }

    /// RTS, the name, then padding so the next candidate cannot join them.
    fn routine(name: &str) -> Vec<u8> {
        let mut bytes = encoded(0x4E75, name);
        bytes.extend_from_slice(&[0x4E, 0x71]); // NOP padding
        bytes
    }

    #[test]
    fn scan_reports_truncation_when_the_entry_limit_is_reached() {
        let mut runner = guest_runner();
        let mut data = Vec::new();
        data.extend_from_slice(&routine("Alpha"));
        data.extend_from_slice(&routine("Beta"));
        data.extend_from_slice(&routine("Gamma"));
        runner.bus_mut().write_bytes(0, &data);
        let (symbols, truncated) =
            scan(&runner, 0, data.len() as u64, 2, u64::MAX).unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Alpha");
        assert_eq!(symbols[1].name, "Beta");
        assert!(truncated);
    }

    #[test]
    fn scan_reports_truncation_when_the_byte_budget_is_reached() {
        let mut runner = guest_runner();
        let data = routine("Alpha");
        runner.bus_mut().write_bytes(0, &data);
        // Budget too small for even one entry.
        let (symbols, truncated) = scan(&runner, 0, data.len() as u64, 8, 4).unwrap();
        assert!(symbols.is_empty());
        assert!(truncated);
    }

    #[test]
    fn scan_is_not_truncated_when_limits_are_not_reached() {
        let mut runner = guest_runner();
        let data = routine("Alpha");
        runner.bus_mut().write_bytes(0, &data);
        let (symbols, truncated) = scan(&runner, 0, data.len() as u64, 8, u64::MAX).unwrap();
        assert_eq!(symbols.len(), 1);
        assert!(!truncated);
    }

    #[test]
    fn resolve_containing_attributes_the_address_to_the_following_symbol() {
        let mut runner = guest_runner();
        let data = routine("GetAnEvent");
        runner.bus_mut().write_bytes(0, &data);
        match resolve_containing(&runner, 0, 4096).unwrap() {
            SymbolResolution::Contained { symbol, distance } => {
                assert_eq!(symbol.name, "GetAnEvent");
                assert_eq!(symbol.address.offset, 2); // just past the RTS
                assert_eq!(distance, 2);
            }
            other => panic!("expected Contained, got {other:?}"),
        }
    }

    #[test]
    fn resolve_containing_reports_an_unsymbolised_routine_rather_than_inheriting_a_name() {
        let mut runner = guest_runner();
        let mut data = Vec::new();
        // First routine ends without a trailing symbol; the next one is named.
        data.extend_from_slice(&[0x4E, 0x75]); // RTS, routine end, no symbol
        data.extend_from_slice(&[0x4E, 0x71]); // NOP filler
        data.extend_from_slice(&encoded(0x4E75, "Named"));
        runner.bus_mut().write_bytes(0, &data);
        match resolve_containing(&runner, 0, 4096).unwrap() {
            SymbolResolution::Unsymbolised { next } => {
                assert_eq!(next.expect("a later symbol was present").name, "Named");
            }
            other => panic!("expected Unsymbolised, got {other:?}"),
        }
    }

    #[test]
    fn resolve_containing_reports_not_found_when_the_window_has_no_symbols() {
        let mut runner = guest_runner();
        // Only filler that is neither a routine end nor a symbol.
        let data = [0x4Eu8, 0x71, 0x4E, 0x71];
        runner.bus_mut().write_bytes(0, &data);
        assert_eq!(resolve_containing(&runner, 0, 4096).unwrap(),
            SymbolResolution::NotFound);
    }

    #[test]
    fn lookup_returns_every_copy_of_a_name_in_ascending_order() {
        let mut runner = guest_runner();
        let mut data = Vec::new();
        data.extend_from_slice(&routine("GetAnEvent"));
        data.extend_from_slice(&routine("Other"));
        data.extend_from_slice(&routine("GetAnEvent"));
        runner.bus_mut().write_bytes(0, &data);
        let found = lookup(&runner, 0, data.len() as u64, "GetAnEvent").unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].address.offset < found[1].address.offset);
        assert!(lookup(&runner, 0, data.len() as u64, "Missing").unwrap().is_empty());
    }

    #[test]
    fn an_ordinary_epilogue_is_not_treated_as_an_unnamed_routine_end() {
        // UNLK A6 immediately precedes RTS in a normal return. It is accepted
        // before a symbol, but must not count as a routine ending unnamed, or
        // every symbolised routine would be reported as unsymbolised.
        assert!(!ROUTINE_ENDS.contains(&0x4E5E));
        assert!(TERMINATORS.contains(&0x4E5E));
    }

    #[test]
    fn truncated_name_at_the_end_of_the_buffer_is_not_a_symbol() {
        let mut data = encoded(0x4E75, "GetAnEvent");
        data.truncate(data.len() - 4);
        assert_eq!(symbol_at(&data, 2), None);
    }

    #[test]
    fn probing_one_past_the_end_of_the_buffer_is_not_a_symbol() {
        // `resolve_containing` probes `offset + 2` after a routine end, which is
        // exactly `data.len()` when that end sits in the final two bytes.
        let data = 0x4E75u16.to_be_bytes();
        assert_eq!(symbol_at(&data, data.len()), None);
        assert_eq!(symbol_at(&data, data.len() + 1), None);
    }

    #[test]
    fn resolve_containing_handles_a_window_ending_at_a_return_instruction() {
        let mut runner = guest_runner();
        runner
            .bus_mut()
            .write_bytes(0x20000, &0x4E75u16.to_be_bytes());
        // The window is exactly the RTS, so the scan probes for a symbol one
        // past the end. It must report the unnamed routine instead of panicking.
        match resolve_containing(&runner, 0x20000, 2).unwrap() {
            SymbolResolution::Unsymbolised { next: None } => {}
            other => panic!("expected Unsymbolised without a next symbol, got {other:?}"),
        }
    }

    #[test]
    fn resolve_containing_handles_a_read_truncated_at_the_end_of_mapped_ram() {
        let mut runner = guest_runner();
        let ram_size = u64::from(runner.bus().ram_size());
        // RTS as the last mapped instruction, then ask for a window that runs
        // past the end of RAM so the read itself is truncated.
        runner
            .bus_mut()
            .write_bytes(ram_size as u32 - 2, &0x4E75u16.to_be_bytes());
        match resolve_containing(&runner, ram_size - 4, 8).unwrap() {
            SymbolResolution::Unsymbolised { next: None } => {}
            other => panic!("expected Unsymbolised without a next symbol, got {other:?}"),
        }
    }
}
