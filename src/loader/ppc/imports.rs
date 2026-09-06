//! CFM imported symbol binding, trace, and probe records.

use super::sprockets::{PpcDrawSprocketTraceEntry, PpcInputSprocketSimpleStateTraceEntry};
use super::PpcImportDispatcherTarget;
use crate::cfm::{CfmSymbolBindings, CfmSymbolError};
use crate::loader::pef::PefResolvedImport;
use crate::trap::dispatch::key_map_key_is_down;
use ppc::{PpcFetchHistogram, PpcRunResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcImportBinding {
    pub library_index: u32,
    pub symbol_index: u32,
    pub library_name: String,
    pub symbol_name: String,
    pub class: u8,
    pub weak: bool,
    pub address: u32,
    pub tvector_address: Option<u32>,
    pub trap_pc: u32,
    pub dispatcher_target: PpcImportDispatcherTarget,
}

pub(super) trait PpcImportBindingPolicy {
    fn dispatcher_target(&self, library: &str, symbol: &str) -> PpcImportDispatcherTarget;

    fn fixed_data_address(&self, library: &str, symbol: &str) -> Option<u32>;

    fn is_explicit_hle_library(&self, library: &str) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcImportLayout {
    pub(super) capacity: u32,
    pub(super) tvector_base: u32,
    pub(super) trap_base: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PpcImportBindingError {
    SymbolIndexOutOfRange {
        symbol_index: u32,
        import_count: u32,
    },
    CapacityExceeded {
        import_count: u32,
        capacity: u32,
    },
    CountOverflow,
    BindingAddressOverflow,
    AddressTableOutOfRange,
    RegistryChanged,
}

#[derive(Debug)]
pub(super) struct PpcImportBindingPlan {
    start_count: u32,
    next_count: u32,
    bindings: Vec<PpcImportBinding>,
    relocation_addresses: Vec<u32>,
}

impl PpcImportBindingPlan {
    pub(super) fn prepare(
        imports: Vec<PefResolvedImport>,
        local_import_count: usize,
        start_count: u32,
        layout: PpcImportLayout,
        policy: &impl PpcImportBindingPolicy,
    ) -> Result<Self, PpcImportBindingError> {
        let local_count =
            u32::try_from(local_import_count).map_err(|_| PpcImportBindingError::CountOverflow)?;
        let next_count = start_count
            .checked_add(local_count)
            .ok_or(PpcImportBindingError::CountOverflow)?;
        if next_count > layout.capacity {
            return Err(PpcImportBindingError::CapacityExceeded {
                import_count: next_count,
                capacity: layout.capacity,
            });
        }

        let mut bindings = Vec::with_capacity(imports.len());
        for import in imports {
            let local_index = usize::try_from(import.symbol_index)
                .map_err(|_| PpcImportBindingError::BindingAddressOverflow)?;
            if local_index >= local_import_count {
                return Err(PpcImportBindingError::SymbolIndexOutOfRange {
                    symbol_index: import.symbol_index,
                    import_count: local_count,
                });
            }
            let symbol_index = start_count
                .checked_add(import.symbol_index)
                .ok_or(PpcImportBindingError::BindingAddressOverflow)?;
            let trap_pc = checked_slot_address(layout.trap_base, symbol_index, 4)?;
            let mut dispatcher_target =
                policy.dispatcher_target(&import.library_name, &import.symbol_name);
            // PowerPC System Software (1994), pp. 1-25--1-26: an unavailable
            // soft import receives kUnresolvedSymbolAddress rather than a thunk.
            let unresolved_weak =
                import.weak && dispatcher_target == PpcImportDispatcherTarget::Unsupported;
            let fixed_data_address =
                policy.fixed_data_address(&import.library_name, &import.symbol_name);
            let address = if unresolved_weak {
                dispatcher_target = PpcImportDispatcherTarget::UnresolvedWeak;
                0
            } else if let Some(address) = fixed_data_address {
                address
            } else {
                match import.class {
                    2 => checked_slot_address(layout.tvector_base, symbol_index, 8)?,
                    0 | 4 => trap_pc,
                    _ => 0,
                }
            };
            bindings.push(PpcImportBinding {
                library_index: import.library_index,
                symbol_index,
                library_name: import.library_name,
                symbol_name: import.symbol_name,
                class: import.class,
                weak: import.weak,
                address,
                tvector_address: (import.class == 2
                    && !unresolved_weak
                    && fixed_data_address.is_none())
                .then_some(address),
                trap_pc,
                dispatcher_target,
            });
        }

        // Preserve the existing relocation-address projection: its first
        // binding establishes the local base and duplicate indices use the
        // last address written into the projection.
        let mut relocation_addresses = vec![0; local_import_count];
        let address_index_base = bindings
            .first()
            .map_or(start_count, |binding| binding.symbol_index);
        for binding in &bindings {
            let local_index = binding
                .symbol_index
                .checked_sub(address_index_base)
                .and_then(|index| usize::try_from(index).ok())
                .ok_or(PpcImportBindingError::AddressTableOutOfRange)?;
            let address = relocation_addresses
                .get_mut(local_index)
                .ok_or(PpcImportBindingError::AddressTableOutOfRange)?;
            *address = binding.address;
        }

        Ok(Self {
            start_count,
            next_count,
            bindings,
            relocation_addresses,
        })
    }

    pub(super) fn relocation_addresses(&self) -> &[u32] {
        &self.relocation_addresses
    }

    pub(super) fn into_initial_bindings(self) -> Vec<PpcImportBinding> {
        self.bindings
    }
}

fn checked_slot_address(
    base: u32,
    symbol_index: u32,
    stride: u32,
) -> Result<u32, PpcImportBindingError> {
    base.checked_add(
        symbol_index
            .checked_mul(stride)
            .ok_or(PpcImportBindingError::BindingAddressOverflow)?,
    )
    .ok_or(PpcImportBindingError::BindingAddressOverflow)
}

#[derive(Debug)]
pub(super) struct PpcImportRunState {
    bindings: Vec<PpcImportBinding>,
    count: u32,
    indices: Vec<Option<usize>>,
    layout: PpcImportLayout,
}

impl PpcImportRunState {
    pub(super) fn from_parts(
        bindings: Vec<PpcImportBinding>,
        count: u32,
        layout: PpcImportLayout,
    ) -> Self {
        let indices = binding_indices(&bindings, count);
        Self {
            bindings,
            count,
            indices,
            layout,
        }
    }

    pub(super) fn into_parts(self) -> (Vec<PpcImportBinding>, u32) {
        (self.bindings, self.count)
    }

    pub(super) fn binding_cloned(&self, symbol_index: u32) -> Option<PpcImportBinding> {
        let binding_index = usize::try_from(symbol_index)
            .ok()
            .and_then(|index| self.indices.get(index).copied().flatten())?;
        self.bindings.get(binding_index).cloned()
    }

    pub(super) fn bindings(&self) -> &[PpcImportBinding] {
        &self.bindings
    }

    pub(super) fn total_count(&self) -> u32 {
        self.count
    }

    pub(super) fn plan_resolved(
        &self,
        imports: Vec<PefResolvedImport>,
        local_import_count: usize,
        policy: &impl PpcImportBindingPolicy,
    ) -> Result<PpcImportBindingPlan, PpcImportBindingError> {
        PpcImportBindingPlan::prepare(imports, local_import_count, self.count, self.layout, policy)
    }

    pub(super) fn stage_append(
        &mut self,
        plan: PpcImportBindingPlan,
    ) -> Result<PendingImportAppend<'_>, PpcImportBindingError> {
        if plan.start_count != self.count {
            return Err(PpcImportBindingError::RegistryChanged);
        }
        let next_count =
            usize::try_from(plan.next_count).map_err(|_| PpcImportBindingError::CountOverflow)?;
        // Reserve before guest-memory publication. Like the Vec operations this
        // replaces, allocation failure retains Rust's process-level OOM behavior
        // and does not introduce a new Macintosh error result.
        self.bindings.reserve(plan.bindings.len());
        let mut next_indices = vec![None; next_count];
        for (binding_index, binding) in self.bindings.iter().chain(&plan.bindings).enumerate() {
            let Ok(symbol_index) = usize::try_from(binding.symbol_index) else {
                continue;
            };
            if let Some(slot) = next_indices.get_mut(symbol_index) {
                slot.get_or_insert(binding_index);
            }
        }
        Ok(PendingImportAppend {
            run_state: self,
            bindings: plan.bindings,
            next_count: plan.next_count,
            next_indices,
            relocation_addresses: plan.relocation_addresses,
        })
    }

    pub(super) fn symbol_binding_operation<'a, P: PpcImportBindingPolicy + ?Sized>(
        &'a mut self,
        policy: &'a P,
    ) -> PpcCfmSymbolBindingOperation<'a, P> {
        PpcCfmSymbolBindingOperation {
            run_state: self,
            policy,
            pending: None,
        }
    }

    fn commit_symbol_binding(&mut self, pending: PendingSymbolBinding) {
        self.bindings.push(pending.binding);
        self.count += 1;
        self.indices = pending.next_indices;
    }
}

fn binding_indices(bindings: &[PpcImportBinding], count: u32) -> Vec<Option<usize>> {
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    let mut indices = vec![None; count];
    for (binding_index, binding) in bindings.iter().enumerate() {
        let Ok(symbol_index) = usize::try_from(binding.symbol_index) else {
            continue;
        };
        if let Some(slot) = indices.get_mut(symbol_index) {
            slot.get_or_insert(binding_index);
        }
    }
    indices
}

pub(super) struct PendingImportAppend<'a> {
    run_state: &'a mut PpcImportRunState,
    bindings: Vec<PpcImportBinding>,
    next_count: u32,
    next_indices: Vec<Option<usize>>,
    relocation_addresses: Vec<u32>,
}

impl PendingImportAppend<'_> {
    pub(super) fn relocation_addresses(&self) -> &[u32] {
        &self.relocation_addresses
    }

    pub(super) fn commit(mut self) {
        self.run_state.bindings.append(&mut self.bindings);
        self.run_state.count = self.next_count;
        self.run_state.indices = self.next_indices;
    }
}

struct PendingSymbolBinding {
    binding: PpcImportBinding,
    next_indices: Vec<Option<usize>>,
}

pub(super) struct PpcCfmSymbolBindingOperation<'a, P: ?Sized> {
    run_state: &'a mut PpcImportRunState,
    policy: &'a P,
    pending: Option<PendingSymbolBinding>,
}

impl<P: PpcImportBindingPolicy + ?Sized> CfmSymbolBindings for PpcCfmSymbolBindingOperation<'_, P> {
    fn prepare(&mut self, library: &str, symbol: &str) -> Result<(u32, u8), CfmSymbolError> {
        self.pending = None;
        if !self.policy.is_explicit_hle_library(library) {
            return Err(CfmSymbolError::SymbolNotFound);
        }
        if let Some(address) = self.policy.fixed_data_address(library, symbol) {
            return Ok((address, 1));
        }
        if let Some(binding) = self.run_state.bindings.iter().find(|binding| {
            binding.library_name == library
                && binding.symbol_name == symbol
                && binding.class == 2
                && binding.dispatcher_target != PpcImportDispatcherTarget::Unsupported
        }) {
            return Ok((binding.address, binding.class));
        }
        let dispatcher_target = self.policy.dispatcher_target(library, symbol);
        if dispatcher_target == PpcImportDispatcherTarget::Unsupported
            || self.run_state.count >= self.run_state.layout.capacity
        {
            return Err(CfmSymbolError::SymbolNotFound);
        }
        let symbol_index = self.run_state.count;
        let address = checked_slot_address(self.run_state.layout.tvector_base, symbol_index, 8)
            .map_err(|_| CfmSymbolError::NoAddressSpace)?;
        let trap_pc = checked_slot_address(self.run_state.layout.trap_base, symbol_index, 4)
            .map_err(|_| CfmSymbolError::NoAddressSpace)?;

        // Perform host allocation before CFM publishes the guest outputs. This
        // retains the existing process-level OOM behavior and leaves commit as
        // value-only publication.
        self.run_state.bindings.reserve(1);
        let next_count = symbol_index + 1;
        let mut next_indices = self.run_state.indices.clone();
        next_indices.resize(next_count as usize, None);
        next_indices[symbol_index as usize] = Some(self.run_state.bindings.len());
        self.pending = Some(PendingSymbolBinding {
            binding: PpcImportBinding {
                library_index: u32::MAX,
                symbol_index,
                library_name: library.into(),
                symbol_name: symbol.into(),
                class: 2,
                weak: false,
                address,
                tvector_address: Some(address),
                trap_pc,
                dispatcher_target,
            },
            next_indices,
        });
        Ok((address, 2))
    }

    fn commit(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.run_state.commit_symbol_binding(pending);
    }
}

pub(super) struct PpcPersistedSymbolBindings<'a, P: ?Sized> {
    bindings: &'a mut Vec<PpcImportBinding>,
    count: &'a mut u32,
    run_state: Option<PpcImportRunState>,
    layout: PpcImportLayout,
    policy: &'a P,
    pending: Option<PendingSymbolBinding>,
}

impl<'a, P: PpcImportBindingPolicy + ?Sized> PpcPersistedSymbolBindings<'a, P> {
    pub(super) fn new(
        bindings: &'a mut Vec<PpcImportBinding>,
        count: &'a mut u32,
        layout: PpcImportLayout,
        policy: &'a P,
    ) -> Self {
        Self {
            bindings,
            count,
            run_state: None,
            layout,
            policy,
            pending: None,
        }
    }
}

impl<P: PpcImportBindingPolicy + ?Sized> CfmSymbolBindings for PpcPersistedSymbolBindings<'_, P> {
    fn prepare(&mut self, library: &str, symbol: &str) -> Result<(u32, u8), CfmSymbolError> {
        if self.run_state.is_none() {
            self.run_state = Some(PpcImportRunState::from_parts(
                std::mem::take(self.bindings),
                *self.count,
                self.layout,
            ));
        }
        let run_state = self
            .run_state
            .as_mut()
            .expect("persisted run state present");
        let mut operation = run_state.symbol_binding_operation(self.policy);
        let result = operation.prepare(library, symbol);
        self.pending = operation.pending.take();
        result
    }

    fn commit(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let run_state = self
            .run_state
            .as_mut()
            .expect("persisted run state present");
        run_state.commit_symbol_binding(pending);
    }
}

impl<P: ?Sized> Drop for PpcPersistedSymbolBindings<'_, P> {
    fn drop(&mut self) {
        if let Some(run_state) = self.run_state.take() {
            let (bindings, count) = run_state.into_parts();
            *self.bindings = bindings;
            *self.count = count;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcStartupProbe {
    pub result: PpcRunResult,
    pub first_import_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcHleImportTraceEntry {
    pub import_index: u32,
    pub library_name: String,
    pub symbol_name: String,
    pub pc: u32,
    pub lr: u32,
    pub rtoc: u32,
    pub sp: u32,
    pub dispatcher_target: PpcImportDispatcherTarget,
    pub repeat_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcHleRunProbe {
    pub result: PpcRunResult,
    pub handled_import_count: u32,
    pub last_import_index: Option<u32>,
    pub unsupported_import_index: Option<u32>,
    pub import_trace: Vec<PpcHleImportTraceEntry>,
    pub draw_sprocket_trace: Vec<PpcDrawSprocketTraceEntry>,
    pub input_sprocket_trace: Vec<PpcInputSprocketSimpleStateTraceEntry>,
    pub fetch_histogram: Option<PpcFetchHistogram>,
}

pub use crate::cfm::{
    CfmConnection as PpcCfmConnection, CfmExport as PpcCfmExport,
    CfmLibraryFragment as PpcCfmLibraryFragment, CfmState as PpcCfmState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PpcInputSnapshot {
    pub key_map: [u8; 16],
    pub mouse_button: bool,
    pub mouse_v: i16,
    pub mouse_h: i16,
}

impl PpcInputSnapshot {
    pub fn key_down(&self, key_code: u8) -> bool {
        key_map_key_is_down(&self.key_map, key_code)
    }

    pub fn any_key_down(&self, key_codes: &[u8]) -> bool {
        key_codes.iter().copied().any(|key| self.key_down(key))
    }

    pub fn is_idle(&self) -> bool {
        !self.mouse_button && self.key_map == [0; 16]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: PpcImportLayout = PpcImportLayout {
        capacity: 8,
        tvector_base: 0x0100_0000,
        trap_base: 0x0200_0000,
    };

    struct TestPolicy;

    impl PpcImportBindingPolicy for TestPolicy {
        fn dispatcher_target(&self, _library: &str, symbol: &str) -> PpcImportDispatcherTarget {
            match symbol {
                "TickCount" => PpcImportDispatcherTarget::TickCount,
                "NewThread" => PpcImportDispatcherTarget::NewThread,
                _ => PpcImportDispatcherTarget::Unsupported,
            }
        }

        fn fixed_data_address(&self, library: &str, symbol: &str) -> Option<u32> {
            (library == "StdCLib" && symbol == "errno").then_some(0x0300_0000)
        }

        fn is_explicit_hle_library(&self, library: &str) -> bool {
            matches!(library, "InterfaceLib" | "StdCLib")
        }
    }

    fn resolved(
        symbol_index: u32,
        library: &str,
        symbol: &str,
        class: u8,
        weak: bool,
    ) -> PefResolvedImport {
        PefResolvedImport {
            library_index: 7,
            symbol_index,
            library_name: library.into(),
            symbol_name: symbol.into(),
            class,
            weak,
        }
    }

    fn binding(
        symbol_index: u32,
        symbol: &str,
        target: PpcImportDispatcherTarget,
    ) -> PpcImportBinding {
        PpcImportBinding {
            library_index: 0,
            symbol_index,
            library_name: "InterfaceLib".into(),
            symbol_name: symbol.into(),
            class: 2,
            weak: false,
            address: LAYOUT.tvector_base + symbol_index * 8,
            tvector_address: Some(LAYOUT.tvector_base + symbol_index * 8),
            trap_pc: LAYOUT.trap_base + symbol_index * 4,
            dispatcher_target: target,
        }
    }

    #[test]
    fn import_binding_plan_preserves_symbol_classes_weak_imports_and_addresses() {
        let plan = PpcImportBindingPlan::prepare(
            vec![
                resolved(0, "OptionalLib", "Missing", 2, true),
                resolved(1, "StdCLib", "errno", 2, false),
                resolved(2, "InterfaceLib", "TickCount", 2, false),
                resolved(3, "InterfaceLib", "TickCount", 0, false),
            ],
            4,
            0,
            LAYOUT,
            &TestPolicy,
        )
        .unwrap();

        assert_eq!(
            plan.relocation_addresses(),
            &[0, 0x0300_0000, 0x0100_0010, 0x0200_000c]
        );
        let bindings = plan.into_initial_bindings();
        assert_eq!(bindings.len(), 4);
        assert_eq!(
            bindings[0].dispatcher_target,
            PpcImportDispatcherTarget::UnresolvedWeak
        );
        assert_eq!(bindings[0].address, 0);
        assert_eq!(bindings[0].tvector_address, None);
        assert_eq!(bindings[1].address, 0x0300_0000);
        assert_eq!(bindings[1].tvector_address, None);
        assert_eq!(bindings[2].tvector_address, Some(0x0100_0010));
        assert_eq!(bindings[3].address, 0x0200_000c);
        assert_eq!(bindings[3].trap_pc, 0x0200_000c);
        assert_eq!(bindings[3].library_index, 7);
    }

    #[test]
    fn import_binding_plan_rejects_capacity_indexes_and_count_overflow() {
        assert_eq!(
            PpcImportBindingPlan::prepare(
                vec![resolved(1, "InterfaceLib", "TickCount", 2, false)],
                1,
                0,
                LAYOUT,
                &TestPolicy,
            )
            .unwrap_err(),
            PpcImportBindingError::SymbolIndexOutOfRange {
                symbol_index: 1,
                import_count: 1,
            }
        );
        assert_eq!(
            PpcImportBindingPlan::prepare(Vec::new(), 2, 7, LAYOUT, &TestPolicy).unwrap_err(),
            PpcImportBindingError::CapacityExceeded {
                import_count: 9,
                capacity: 8,
            }
        );
        let maximum_capacity = PpcImportLayout {
            capacity: u32::MAX,
            ..LAYOUT
        };
        assert_eq!(
            PpcImportBindingPlan::prepare(Vec::new(), 1, u32::MAX, maximum_capacity, &TestPolicy,)
                .unwrap_err(),
            PpcImportBindingError::CountOverflow
        );
    }

    #[test]
    fn import_binding_plan_rejects_binding_address_overflow() {
        let overflowing = PpcImportLayout {
            capacity: u32::MAX,
            tvector_base: u32::MAX - 3,
            trap_base: 0,
        };
        assert_eq!(
            PpcImportBindingPlan::prepare(
                vec![resolved(0, "InterfaceLib", "TickCount", 2, false)],
                1,
                1,
                overflowing,
                &TestPolicy,
            )
            .unwrap_err(),
            PpcImportBindingError::BindingAddressOverflow
        );
    }

    #[test]
    fn relocation_projection_uses_the_last_duplicate_address() {
        let plan = PpcImportBindingPlan::prepare(
            vec![
                resolved(0, "InterfaceLib", "TickCount", 2, false),
                resolved(0, "StdCLib", "errno", 1, false),
            ],
            2,
            0,
            LAYOUT,
            &TestPolicy,
        )
        .unwrap();

        assert_eq!(plan.relocation_addresses(), &[0x0300_0000, 0]);
    }

    #[test]
    fn import_run_state_preserves_sparse_first_binding_lookup() {
        let first = binding(2, "TickCount", PpcImportDispatcherTarget::TickCount);
        let mut duplicate = binding(2, "Duplicate", PpcImportDispatcherTarget::Unsupported);
        duplicate.address = 0;
        let outside = binding(9, "Outside", PpcImportDispatcherTarget::Unsupported);
        let state =
            PpcImportRunState::from_parts(vec![first.clone(), duplicate, outside], 4, LAYOUT);

        assert_eq!(state.binding_cloned(2), Some(first));
        assert_eq!(state.binding_cloned(0), None);
        assert_eq!(state.binding_cloned(9), None);
    }

    #[test]
    fn pending_import_append_is_infallible_after_staging_and_drop_is_inert() {
        let original = binding(0, "TickCount", PpcImportDispatcherTarget::TickCount);
        let mut state = PpcImportRunState::from_parts(vec![original.clone()], 1, LAYOUT);
        let plan = state
            .plan_resolved(
                vec![resolved(0, "InterfaceLib", "NewThread", 2, false)],
                1,
                &TestPolicy,
            )
            .unwrap();
        {
            let pending = state.stage_append(plan).unwrap();
            assert_eq!(pending.relocation_addresses(), &[LAYOUT.tvector_base + 8]);
        }
        assert_eq!(state.total_count(), 1);
        assert_eq!(state.binding_cloned(0), Some(original));
        assert_eq!(state.binding_cloned(1), None);

        let plan = state
            .plan_resolved(
                vec![resolved(0, "InterfaceLib", "NewThread", 2, false)],
                1,
                &TestPolicy,
            )
            .unwrap();
        let pending = state.stage_append(plan).unwrap();
        let (): () = pending.commit();
        assert_eq!(state.total_count(), 2);
        assert_eq!(
            state.binding_cloned(1).map(|binding| binding.symbol_name),
            Some("NewThread".into())
        );
        let (bindings, count) = state.into_parts();
        assert_eq!(count, 2);
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn staged_append_rebuild_activates_old_sparse_first_binding() {
        let old = binding(2, "OldSparse", PpcImportDispatcherTarget::TickCount);
        let mut state = PpcImportRunState::from_parts(vec![old.clone()], 1, LAYOUT);
        assert_eq!(state.binding_cloned(2), None);
        let plan = state
            .plan_resolved(
                vec![
                    resolved(0, "InterfaceLib", "TickCount", 2, false),
                    resolved(1, "InterfaceLib", "NewThread", 2, false),
                ],
                2,
                &TestPolicy,
            )
            .unwrap();
        let pending = state.stage_append(plan).unwrap();
        let (): () = pending.commit();

        assert_eq!(state.total_count(), 3);
        assert_eq!(
            state.binding_cloned(1).map(|binding| binding.symbol_name),
            Some("TickCount".into())
        );
        assert_eq!(state.binding_cloned(2), Some(old));
    }

    #[test]
    fn staged_append_refuses_a_changed_registry_before_publication() {
        let plan = PpcImportBindingPlan::prepare(
            vec![resolved(0, "InterfaceLib", "TickCount", 2, false)],
            1,
            0,
            LAYOUT,
            &TestPolicy,
        )
        .unwrap();
        let mut state = PpcImportRunState::from_parts(
            vec![binding(
                0,
                "TickCount",
                PpcImportDispatcherTarget::TickCount,
            )],
            1,
            LAYOUT,
        );
        assert!(matches!(
            state.stage_append(plan),
            Err(PpcImportBindingError::RegistryChanged)
        ));
        assert_eq!(state.total_count(), 1);
    }

    #[test]
    fn cfm_symbol_binding_staging_is_scoped_to_one_operation() {
        let existing = binding(0, "TickCount", PpcImportDispatcherTarget::TickCount);
        let mut state = PpcImportRunState::from_parts(vec![existing.clone()], 1, LAYOUT);
        {
            let mut operation = state.symbol_binding_operation(&TestPolicy);
            assert_eq!(
                operation.prepare("InterfaceLib", "TickCount"),
                Ok((existing.address, 2))
            );
            operation.commit();
        }
        assert_eq!(state.total_count(), 1);

        {
            let mut operation = state.symbol_binding_operation(&TestPolicy);
            assert_eq!(
                operation.prepare("InterfaceLib", "NewThread"),
                Ok((LAYOUT.tvector_base + 8, 2))
            );
        }
        assert_eq!(state.total_count(), 1);
        assert_eq!(state.binding_cloned(1), None);

        {
            let mut operation = state.symbol_binding_operation(&TestPolicy);
            assert_eq!(
                operation.prepare("InterfaceLib", "NewThread"),
                Ok((0x0100_0008, 2))
            );
            assert_eq!(
                operation.prepare("InterfaceLib", "Unknown"),
                Err(CfmSymbolError::SymbolNotFound)
            );
            operation.commit();
        }
        assert_eq!(state.total_count(), 1);

        {
            let mut operation = state.symbol_binding_operation(&TestPolicy);
            assert_eq!(operation.prepare("StdCLib", "errno"), Ok((0x0300_0000, 1)));
            operation.commit();
        }
        assert_eq!(state.total_count(), 1);

        {
            let mut operation = state.symbol_binding_operation(&TestPolicy);
            assert_eq!(
                operation.prepare("InterfaceLib", "NewThread"),
                Ok((0x0100_0008, 2))
            );
            operation.commit();
        }
        assert_eq!(state.total_count(), 2);
        assert_eq!(
            state.binding_cloned(1).map(|binding| binding.symbol_name),
            Some("NewThread".into())
        );
    }

    #[test]
    fn cfm_symbol_binding_reuses_existing_gateway_at_capacity() {
        let existing = binding(0, "TickCount", PpcImportDispatcherTarget::TickCount);
        let mut state =
            PpcImportRunState::from_parts(vec![existing.clone()], LAYOUT.capacity, LAYOUT);
        let mut operation = state.symbol_binding_operation(&TestPolicy);

        assert_eq!(
            operation.prepare("InterfaceLib", "TickCount"),
            Ok((existing.address, 2))
        );
        assert_eq!(
            operation.prepare("InterfaceLib", "NewThread"),
            Err(CfmSymbolError::SymbolNotFound)
        );
        assert_eq!(
            operation.prepare("UnknownLib", "TickCount"),
            Err(CfmSymbolError::SymbolNotFound)
        );
    }

    #[test]
    fn persisted_symbol_binding_provider_restores_commits_and_drops_staging() {
        let existing = binding(0, "TickCount", PpcImportDispatcherTarget::TickCount);
        let mut bindings = vec![existing];
        let mut count = 1;
        {
            let provider =
                PpcPersistedSymbolBindings::new(&mut bindings, &mut count, LAYOUT, &TestPolicy);
            assert!(provider.run_state.is_none());
        }
        assert_eq!(count, 1);
        assert_eq!(bindings.len(), 1);
        {
            let mut provider =
                PpcPersistedSymbolBindings::new(&mut bindings, &mut count, LAYOUT, &TestPolicy);
            assert_eq!(
                provider.prepare("InterfaceLib", "NewThread"),
                Ok((LAYOUT.tvector_base + 8, 2))
            );
        }
        assert_eq!(count, 1);
        assert_eq!(bindings.len(), 1);

        {
            let mut provider =
                PpcPersistedSymbolBindings::new(&mut bindings, &mut count, LAYOUT, &TestPolicy);
            assert_eq!(
                provider.prepare("InterfaceLib", "NewThread"),
                Ok((LAYOUT.tvector_base + 8, 2))
            );
            provider.commit();
        }
        assert_eq!(count, 2);
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[1].symbol_name, "NewThread");
    }
}
