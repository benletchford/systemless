//! Generated Trap Manager code and machine-profile topology.
//!
//! These identities describe system code, never application patches. The
//! memory owner supplies allocation/protection; no CPU or dispatcher is needed.

use super::manager::{raw_trap_route, RawTrapRoute, OS_TRAP_TABLE_SLOTS, TOOLBOX_TRAP_TABLE_SLOTS};
use std::collections::{HashMap, HashSet};

/// Trap-table topology selected for the emulated Mac OS 8.1 machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapTableProfile {
    M68k68040,
    PowerPc604,
}

/// Profile-specific classification layered over the generated raw-word map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProfileTrapRoute {
    pub(crate) raw: RawTrapRoute,
    pub(crate) default_is_unimplemented: bool,
    pub(crate) has_permanent_come_from: bool,
    pub(crate) default_gateway_word: u16,
}

// Permanent come-from heads observed in the selected Mac OS 8.1 profiles.
// Trap Manager APIs expose and replace the successor behind each head, while
// the raw low-memory table continues to contain the head itself. IM:OSUtils
// (1994), pp. 8-8--8-9 and 8-25--8-31.
pub(crate) const M68K_68040_COME_FROM_TRAPS: &[u16] = &[
    0xA002, 0xA003, 0xA008, 0xA00A, 0xA012, 0xA023, 0xA024, 0xA030, 0xA031, 0xA054, 0xA078, 0xA823,
    0xA869, 0xA873, 0xA879, 0xA88B, 0xA893, 0xA89C, 0xA8A1, 0xA8B5, 0xA8CF, 0xA8E4, 0xA8E5, 0xA8EA,
    0xA8EC, 0xA905, 0xA908, 0xA909, 0xA90A, 0xA90C, 0xA90D, 0xA91E, 0xA924, 0xA956, 0xA972, 0xA999,
    0xA9A0, 0xA9A2, 0xA9C9, 0xA9DC, 0xA9E1, 0xA9ED, 0xA9EF, 0xAA00, 0xAA1F, 0xAA27, 0xAA43, 0xAA4E,
    0xAAFB,
];

pub(crate) const POWERPC_604_COME_FROM_TRAPS: &[u16] = &[0xA823, 0xA851, 0xA996, 0xA999, 0xAAFB];

// Reviewed default-address aliases observed repeatedly in the selected 68040
// profile. Universal Interfaces 3.4 assigns CloseCPort to $AA02 while Inside
// Macintosh Volume V, V-72/V-291 records its earlier $A87D entry. The same
// profile also shares the compound-handle disposal routine used by
// DisposePixPat and DisposeCCursor; IM:V V-55/V-63 shows their common leading
// map/data/expanded-data/expanded-map layout. The selected 604 profile exposes
// distinct addresses for all four slots, so these aliases are profile-local.
const M68K_68040_DEFAULT_GATEWAY_ALIASES: &[(u16, u16)] = &[(0xAA02, 0xA87D), (0xAA26, 0xAA08)];

// Both selected Mac OS 8.1 profiles identify the modern 1,024-entry Toolbox
// table's `$AA6E` slot as the `Unimplemented` logical identity. All other
// captured slots have callable defaults. IM:OSUtils (1994), pp. 8-22 and
// 8-32; IM:Overview (1992), pp. 9-14--9-15.
const MAC_OS_81_UNIMPLEMENTED_TRAPS: &[u16] = &[0xAA6E];

impl TrapTableProfile {
    fn come_from_traps(self) -> &'static [u16] {
        match self {
            Self::M68k68040 => M68K_68040_COME_FROM_TRAPS,
            Self::PowerPc604 => POWERPC_604_COME_FROM_TRAPS,
        }
    }

    fn unimplemented_traps(self) -> &'static [u16] {
        match self {
            Self::M68k68040 | Self::PowerPc604 => MAC_OS_81_UNIMPLEMENTED_TRAPS,
        }
    }

    fn default_gateway_word(self, canonical_word: u16) -> u16 {
        match self {
            Self::M68k68040 => M68K_68040_DEFAULT_GATEWAY_ALIASES
                .iter()
                .find_map(|&(alias, target)| (alias == canonical_word).then_some(target))
                .unwrap_or(canonical_word),
            Self::PowerPc604 => canonical_word,
        }
    }

    pub(crate) fn route(self, trap_word: u16) -> ProfileTrapRoute {
        let raw = *raw_trap_route(trap_word);
        ProfileTrapRoute {
            raw,
            default_is_unimplemented: self.unimplemented_traps().contains(&raw.canonical_word),
            has_permanent_come_from: self.come_from_traps().contains(&raw.canonical_word),
            default_gateway_word: self.default_gateway_word(raw.canonical_word),
        }
    }
}

/// Bounded system-code publication supplied by the memory owner. Profile
/// construction preflights its total allocation through this contract. Individual
/// publication requires reserved capacity. `existing` names
/// code already owned by this service in the same memory lifetime. Publication
/// writes all words and makes new code read-only to ordinary guest stores.
/// This interface does not own an allocator or authorize application writes.
pub(crate) trait TrapCodeMemory {
    fn trap_code_allocation_bucket(&self, size: u32) -> u32;
    fn can_allocate_trap_code(&self, size: u32) -> bool;
    fn publish_trap_code(&mut self, existing: Option<u32>, words: &[u16]) -> u32;
}

/// An inactive process image. Moving its cells into guest RAM activates it;
/// subsequent lookups must read that RAM, not this construction snapshot.
pub(crate) struct TrapTableImage {
    pub(crate) raw_entries: Vec<u32>,
    pub(crate) exception_vectors: [u32; 2],
}

/// System-lifetime identities paired with one code allocation/mapping owner.
/// Process teardown preserves defaults but builds fresh heads and vectors.
/// Moving this value does not move or extend the underlying code storage.
#[derive(Default)]
pub(crate) struct TrapSystemGateways {
    defaults: HashMap<u16, u32>,
}

impl TrapSystemGateways {
    pub(crate) fn get(&self, word: u16) -> Option<u32> {
        self.defaults
            .get(&raw_trap_route(word).canonical_word)
            .copied()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.defaults.is_empty()
    }

    /// Toolbox entries use the auto-pop A-line word; OS entries use the
    /// canonical word and RTS. Saved default calls bypass later patches.
    /// IM:OSUtils (1994), pp. 8-23--8-30; IM:V (1986), p. V-577.
    pub(crate) fn get_or_create(&mut self, memory: &mut impl TrapCodeMemory, word: u16) -> u32 {
        let route = raw_trap_route(word);
        let words = if route.is_toolbox {
            [route.canonical_word | 0x0400, 0]
        } else {
            [route.canonical_word, 0x4E75]
        };
        let words = &words[..if route.is_toolbox { 1 } else { 2 }];
        let address = memory.publish_trap_code(self.get(word), words);
        self.defaults.insert(route.canonical_word, address);
        address
    }

    pub(crate) fn default_gateway(&self, profile: TrapTableProfile, word: u16) -> Option<u32> {
        let route = profile.route(word);
        self.get(if route.default_is_unimplemented {
            0xAA6E
        } else {
            route.default_gateway_word
        })
    }

    /// Required new storage using the memory owner's actual allocation bucket.
    /// Cached default identities cost no allocation; each process gets new
    /// protected heads and exception-vector gateways.
    pub(crate) fn required_bytes(
        &self,
        profile: TrapTableProfile,
        bucket: impl Fn(u32) -> u32,
    ) -> u32 {
        let mut words = HashSet::from([0xAA6E]);
        for word in Self::table_words() {
            let route = profile.route(word);
            if !route.default_is_unimplemented {
                words.insert(route.default_gateway_word);
            }
        }
        words
            .into_iter()
            .filter(|word| self.get(*word).is_none())
            .map(|word| {
                bucket(if raw_trap_route(word).is_toolbox {
                    2
                } else {
                    4
                })
            })
            .sum::<u32>()
            + profile.come_from_traps().len() as u32 * bucket(10)
            + 2 * bucket(2)
    }

    fn table_words() -> impl Iterator<Item = u16> {
        (0..OS_TRAP_TABLE_SLOTS)
            .map(|slot| 0xA000 | slot)
            .chain((0..TOOLBOX_TRAP_TABLE_SLOTS).map(|slot| 0xA800 | slot))
    }

    /// Materialize profile defaults without constructing a manager adapter.
    /// The memory owner preflights the entire allocation before any code,
    /// identity or table image is published. Refusal leaves both owners intact.
    /// IM:OSUtils (1994), pp. 8-4--8-9, 8-25--8-31.
    pub(crate) fn create_table(
        &mut self,
        memory: &mut impl TrapCodeMemory,
        profile: TrapTableProfile,
    ) -> Option<TrapTableImage> {
        let required =
            self.required_bytes(profile, |size| memory.trap_code_allocation_bucket(size));
        if !memory.can_allocate_trap_code(required) {
            return None;
        }
        let unimplemented = self.get_or_create(memory, 0xAA6E);
        let mut raw_entries =
            Vec::with_capacity(usize::from(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS));
        for word in Self::table_words() {
            let route = profile.route(word);
            let default = if route.default_is_unimplemented {
                self.defaults.insert(word, unimplemented);
                unimplemented
            } else {
                self.get_or_create(memory, route.default_gateway_word)
            };
            raw_entries.push(if route.has_permanent_come_from {
                Self::create_come_from_head(memory, default)
            } else {
                default
            });
        }
        // Line-A/F faults stack the faulting PC; RTE retries the instruction.
        let exception_vectors = [
            memory.publish_trap_code(None, &[0x4E73]),
            memory.publish_trap_code(None, &[0x4E73]),
        ];
        Some(TrapTableImage {
            raw_entries,
            exception_vectors,
        })
    }

    pub(crate) fn create_come_from_head(memory: &mut impl TrapCodeMemory, successor: u32) -> u32 {
        // BRA.S body; JMP successor; body: BRA.S exit JMP. The signature and
        // protected provenance jointly identify the mutable exit link.
        memory.publish_trap_code(
            None,
            &[
                0x6006,
                0x4EF9,
                (successor >> 16) as u16,
                successor as u16,
                0x60F8,
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CodeMemory {
        next: u32,
        limit: u32,
        bucket: fn(u32) -> u32,
        code: HashMap<u32, Vec<u16>>,
    }

    impl TrapCodeMemory for CodeMemory {
        fn trap_code_allocation_bucket(&self, size: u32) -> u32 {
            (self.bucket)(size)
        }
        fn can_allocate_trap_code(&self, size: u32) -> bool {
            self.next
                .checked_add(size)
                .is_some_and(|end| end <= self.limit)
        }
        fn publish_trap_code(&mut self, existing: Option<u32>, words: &[u16]) -> u32 {
            let address = existing.unwrap_or_else(|| {
                let address = self.next;
                self.next += (self.bucket)(words.len() as u32 * 2);
                address
            });
            self.code.insert(address, words.to_vec());
            address
        }
    }

    fn four(size: u32) -> u32 {
        (size + 3) & !3
    }
    fn eight(size: u32) -> u32 {
        (size + 7) & !7
    }

    #[test]
    fn profile_construction_accounts_for_memory_and_preserves_system_identity() {
        for bucket in [four as fn(u32) -> u32, eight] {
            for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
                let mut memory = CodeMemory {
                    next: 0x10000,
                    limit: u32::MAX,
                    bucket,
                    code: HashMap::new(),
                };
                let mut system = TrapSystemGateways::default();
                // Pre-existing defaults must be excluded from new allocation.
                let tick = system.get_or_create(&mut memory, 0xAD75);
                let os = system.get_or_create(&mut memory, 0xA31E);
                let before = memory.next;
                let needed = system.required_bytes(profile, bucket);
                let first = system.create_table(&mut memory, profile).unwrap();
                assert_eq!(memory.next - before, needed);
                assert_eq!(first.raw_entries.len(), 1280);
                assert_eq!(system.get(0xA975), Some(tick));
                assert_eq!(system.get(0xA01E), Some(os));
                for (word, &raw) in TrapSystemGateways::table_words().zip(&first.raw_entries) {
                    let route = profile.route(word);
                    let default = system.default_gateway(profile, word).unwrap();
                    if route.has_permanent_come_from {
                        assert_ne!(raw, default);
                        assert_eq!(
                            memory.code[&raw],
                            [
                                0x6006,
                                0x4EF9,
                                (default >> 16) as u16,
                                default as u16,
                                0x60F8
                            ]
                        );
                    } else {
                        assert_eq!(raw, default);
                    }
                    let gateway_word = if route.default_is_unimplemented {
                        0xAA6E
                    } else {
                        route.default_gateway_word
                    };
                    assert_eq!(
                        memory.code[&default],
                        if route.raw.is_toolbox {
                            vec![gateway_word | 0x0400]
                        } else {
                            vec![gateway_word, 0x4E75]
                        }
                    );
                }
                assert_eq!(
                    system.default_gateway(profile, 0xAA02) == system.get(0xA87D),
                    profile == TrapTableProfile::M68k68040
                );
                let before = memory.next;
                let needed = system.required_bytes(profile, bucket);
                let second = system.create_table(&mut memory, profile).unwrap();
                assert_eq!(memory.next - before, needed);
                assert_eq!(
                    needed,
                    profile.come_from_traps().len() as u32 * bucket(10) + 2 * bucket(2)
                );
                for (word, (&old, &new)) in TrapSystemGateways::table_words()
                    .zip(first.raw_entries.iter().zip(&second.raw_entries))
                {
                    assert_eq!(old != new, profile.route(word).has_permanent_come_from);
                }
                for (old, new) in first
                    .exception_vectors
                    .into_iter()
                    .zip(second.exception_vectors)
                {
                    assert_ne!(old, new);
                    assert_eq!(memory.code[&old], [0x4E73]);
                    assert_eq!(memory.code[&new], [0x4E73]);
                }
            }
        }
    }

    #[test]
    fn profile_construction_refuses_capacity_before_publication_and_retries() {
        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            for reused in [false, true] {
                let mut memory = CodeMemory {
                    next: 0x10000,
                    limit: u32::MAX,
                    bucket: eight,
                    code: HashMap::new(),
                };
                let mut system = TrapSystemGateways::default();
                if reused {
                    system.create_table(&mut memory, profile).unwrap();
                }
                let before = memory.next;
                let code = memory.code.clone();
                let defaults = system.defaults.clone();
                let required = system.required_bytes(profile, eight);
                memory.limit = before + required - 1;
                assert!(system.create_table(&mut memory, profile).is_none());
                assert_eq!(memory.next, before);
                assert_eq!(memory.code, code);
                assert_eq!(system.defaults, defaults);
                memory.limit += 1;
                assert!(system.create_table(&mut memory, profile).is_some());
                assert_eq!(memory.next, before + required);
            }
        }
    }

    #[test]
    fn saved_gateway_republication_uses_canonical_identity_without_allocating() {
        let mut memory = CodeMemory {
            next: 0x10000,
            limit: u32::MAX,
            bucket: four,
            code: HashMap::new(),
        };
        let mut system = TrapSystemGateways::default();
        for (first_word, second_word, expected) in [
            (0xAD75, 0xA975, vec![0xAD75]),
            (0xA31E, 0xA01E, vec![0xA01E, 0x4E75]),
        ] {
            let gateway = system.get_or_create(&mut memory, first_word);
            memory.code.insert(gateway, vec![0]);
            let before = memory.next;
            assert_eq!(system.get_or_create(&mut memory, second_word), gateway);
            assert_eq!(memory.next, before);
            assert_eq!(memory.code[&gateway], expected);
        }
    }
}
