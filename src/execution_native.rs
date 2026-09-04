//! Native engine lifetime ownership, independent of host presentation.
//!
//! A checked-out context retains its owner and role. No store borrow survives
//! checkout, so the execution coordinator can suspend native work while a
//! classic callback runs without borrowing the engine bank recursively.

use std::cell::OnceCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeEngineRole {
    Application,
    Companion,
}

enum NativeSlot<T> {
    Empty,
    Installed(T),
    CheckedOut(Rc<OnceCell<T>>),
}

pub(crate) struct NativeExecution<T> {
    identity: Rc<()>,
    application: NativeSlot<T>,
    companion: NativeSlot<T>,
    staged: Option<T>,
}

/// Non-cloneable custody of one installed adapter, including its return slot.
/// Access is scoped to an adapter operation; this is not a borrowed bank lease.
pub(crate) struct NativeContext<T> {
    identity: Rc<()>,
    role: NativeEngineRole,
    adapter: Option<T>,
    return_slot: Rc<OnceCell<T>>,
}

impl<T> NativeContext<T> {
    pub(crate) fn adapter_mut(&mut self) -> &mut T {
        self.adapter.as_mut().expect("context retains its adapter")
    }
}

// The cell is a return channel, not another engine authority. Only the unique
// context can fill it; the slot remains unavailable until custody is returned.
impl<T> Drop for NativeContext<T> {
    fn drop(&mut self) {
        if let Some(adapter) = self.adapter.take() {
            let result = self.return_slot.set(adapter);
            debug_assert!(result.is_ok(), "native context returned more than once");
        }
    }
}

impl<T> NativeSlot<T> {
    fn installed(&self) -> Option<&T> {
        match self {
            Self::Installed(adapter) => Some(adapter),
            Self::CheckedOut(return_slot) => return_slot.get(),
            Self::Empty => None,
        }
    }

    fn checked_out(&self) -> bool {
        matches!(self, Self::CheckedOut(return_slot) if return_slot.get().is_none())
    }

    fn reclaim(&mut self) {
        if matches!(self, Self::CheckedOut(return_slot) if return_slot.get().is_some()) {
            let Self::CheckedOut(return_slot) = std::mem::replace(self, Self::Empty) else {
                unreachable!("returned slot was validated")
            };
            // Drop has released the context's reference before another owner
            // operation can run. No guest or manager code can access this cell.
            let cell = Rc::try_unwrap(return_slot)
                .unwrap_or_else(|_| panic!("returned context still owns its slot"));
            *self = Self::Installed(cell.into_inner().expect("returned adapter"));
        }
    }
}

impl<T> Default for NativeExecution<T> {
    fn default() -> Self {
        Self {
            identity: Rc::new(()),
            application: NativeSlot::Empty,
            companion: NativeSlot::Empty,
            staged: None,
        }
    }
}

impl<T> NativeExecution<T> {
    fn slot(&self, role: NativeEngineRole) -> &NativeSlot<T> {
        match role {
            NativeEngineRole::Application => &self.application,
            NativeEngineRole::Companion => &self.companion,
        }
    }

    fn slot_mut(&mut self, role: NativeEngineRole) -> &mut NativeSlot<T> {
        match role {
            NativeEngineRole::Application => &mut self.application,
            NativeEngineRole::Companion => &mut self.companion,
        }
    }

    pub(crate) fn availability(&self) -> crate::execution_kernel::NativeAvailability {
        crate::execution_kernel::NativeAvailability {
            application: self.application.installed().is_some(),
            companion: self.companion.installed().is_some(),
            staged_companion: self.staged.is_some() && matches!(self.companion, NativeSlot::Empty),
        }
    }

    pub(crate) fn application(&self) -> Option<&T> {
        self.application.installed()
    }

    pub(crate) fn application_mut(&mut self) -> Option<&mut T> {
        self.application.reclaim();
        match &mut self.application {
            NativeSlot::Installed(adapter) => Some(adapter),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn companion(&self) -> Option<&T> {
        self.companion.installed()
    }

    pub(crate) fn install(&mut self, role: NativeEngineRole, adapter: T) -> Result<(), T> {
        if !matches!(self.slot(role), NativeSlot::Empty) {
            return Err(adapter);
        }
        *self.slot_mut(role) = NativeSlot::Installed(adapter);
        Ok(())
    }

    pub(crate) fn take(&mut self, role: NativeEngineRole) -> Option<NativeContext<T>> {
        self.slot_mut(role).reclaim();
        if !matches!(self.slot(role), NativeSlot::Installed(_)) {
            return None;
        }
        let return_slot = Rc::new(OnceCell::new());
        let NativeSlot::Installed(adapter) = std::mem::replace(
            self.slot_mut(role),
            NativeSlot::CheckedOut(Rc::clone(&return_slot)),
        ) else {
            unreachable!("installed slot was validated before checkout")
        };
        Some(NativeContext {
            identity: Rc::clone(&self.identity),
            role,
            adapter: Some(adapter),
            return_slot,
        })
    }

    pub(crate) fn restore(&mut self, context: NativeContext<T>) -> Result<(), NativeContext<T>> {
        if !Rc::ptr_eq(&self.identity, &context.identity)
            || !matches!(self.slot(context.role), NativeSlot::CheckedOut(slot)
                if Rc::ptr_eq(slot, &context.return_slot) && slot.get().is_none())
        {
            return Err(context);
        }
        let role = context.role;
        drop(context);
        self.slot_mut(role).reclaim();
        Ok(())
    }

    pub(crate) fn can_relaunch(&self) -> bool {
        !self.application.checked_out() && !self.companion.checked_out()
    }

    /// Preserve a staged companion for a classic launch, but discard it when
    /// the launch itself supplies the native application adapter.
    pub(crate) fn reset_for_launch(&mut self, native_application: bool) -> bool {
        if !self.can_relaunch() {
            return false;
        }
        self.application = NativeSlot::Empty;
        self.companion = NativeSlot::Empty;
        if native_application {
            self.staged = None;
        }
        true
    }

    pub(crate) fn stage_companion(&mut self, adapter: T) -> Result<(), T> {
        if !matches!(self.companion, NativeSlot::Empty) {
            return Err(adapter);
        }
        self.staged = Some(adapter);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn has_staged_companion(&self) -> bool {
        self.staged.is_some()
    }

    pub(crate) fn take_staged_companion(&mut self) -> Option<T> {
        if !matches!(self.companion, NativeSlot::Empty) {
            return None;
        }
        self.staged.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abandoned_context_returns_mutated_adapter_for_another_checkout() {
        let mut owner = NativeExecution::default();
        for role in [NativeEngineRole::Application, NativeEngineRole::Companion] {
            assert!(owner.install(role, vec![1, 2]).is_ok());
            let mut context = owner.take(role).unwrap();
            context.adapter_mut().push(3);
            assert!(!owner.can_relaunch());
            assert!(owner.take(role).is_none());
            drop(context);
            assert!(owner.can_relaunch());
            let availability = owner.availability();
            assert!(match role {
                NativeEngineRole::Application => availability.application,
                NativeEngineRole::Companion => availability.companion,
            });
            // A returned context is occupied, even before the next checkout.
            assert_eq!(owner.install(role, vec![99]), Err(vec![99]));
            let mut next = owner.take(role).unwrap();
            assert_eq!(next.adapter_mut(), &[1, 2, 3]);
            next.adapter_mut().push(4);
            assert!(owner.restore(next).is_ok());
        }
        assert_eq!(owner.application().unwrap(), &[1, 2, 3, 4]);
        assert_eq!(owner.companion().unwrap(), &[1, 2, 3, 4]);
    }

    #[test]
    fn unwind_returns_native_custody_without_rolling_back_adapter_progress() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let mut owner = NativeExecution::default();
        assert!(owner
            .install(NativeEngineRole::Application, vec![10])
            .is_ok());
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut context = owner.take(NativeEngineRole::Application).unwrap();
            context.adapter_mut().push(20);
            panic!("interrupted coordinator");
        }));
        assert!(result.is_err());
        assert_eq!(owner.application().unwrap(), &[10, 20]);
        owner.application_mut().unwrap().push(30);
        assert_eq!(owner.application().unwrap(), &[10, 20, 30]);
        assert!(owner.reset_for_launch(false));
        assert!(owner.application().is_none());
    }

    #[test]
    fn rejected_restore_and_owner_drop_release_each_adapter_once() {
        use std::cell::Cell;
        struct Adapter(Rc<Cell<usize>>);
        impl Drop for Adapter {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut owner = NativeExecution::default();
        let mut other = NativeExecution::default();
        assert!(owner
            .install(NativeEngineRole::Application, Adapter(Rc::clone(&drops)))
            .is_ok());
        let context = owner.take(NativeEngineRole::Application).unwrap();
        let context = other.restore(context).err().unwrap();
        drop(context);
        assert_eq!(drops.get(), 0);
        assert!(owner.application().is_some());
        assert!(other.application().is_none());
        let context = owner.take(NativeEngineRole::Application).unwrap();
        drop(owner);
        assert_eq!(drops.get(), 0);
        drop(context);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn native_contexts_restore_to_their_original_owner_and_role() {
        let mut owner = NativeExecution::default();
        let mut other = NativeExecution::default();
        assert!(owner.install(NativeEngineRole::Application, 10).is_ok());
        assert!(owner.install(NativeEngineRole::Companion, 20).is_ok());
        assert!(other.install(NativeEngineRole::Application, 30).is_ok());
        let mut application = owner.take(NativeEngineRole::Application).unwrap();
        let companion = owner.take(NativeEngineRole::Companion).unwrap();
        let other_application = other.take(NativeEngineRole::Application).unwrap();
        *application.adapter_mut() = 11;
        let application = other.restore(application).err().unwrap();
        assert!(!owner.can_relaunch());
        assert!(owner.take(NativeEngineRole::Application).is_none());
        assert_eq!(owner.install(NativeEngineRole::Application, 99), Err(99));
        assert!(!owner.reset_for_launch(true));
        assert_eq!(owner.stage_companion(99), Err(99));
        assert!(owner.restore(companion).is_ok());
        assert!(!owner.can_relaunch());
        assert!(owner.restore(application).is_ok());
        assert!(other.restore(other_application).is_ok());
        assert_eq!(owner.application(), Some(&11));
        assert_eq!(owner.companion(), Some(&20));
        assert_eq!(other.application(), Some(&30));
        assert!(owner.can_relaunch());
    }

    #[test]
    fn launch_and_companion_staging_preserve_uncommitted_contexts() {
        let mut owner = NativeExecution::default();
        assert!(owner.stage_companion(40).is_ok());
        assert!(owner.reset_for_launch(false));
        assert!(owner.has_staged_companion());
        let staged = owner.take_staged_companion().unwrap();
        assert!(owner.install(NativeEngineRole::Companion, staged).is_ok());
        let context = owner.take(NativeEngineRole::Companion).unwrap();
        assert!(!owner.reset_for_launch(true));
        assert!(owner.restore(context).is_ok());
        assert_eq!(owner.companion(), Some(&40));
        assert!(owner.reset_for_launch(false));
        assert!(owner.stage_companion(50).is_ok());
        assert!(owner.reset_for_launch(true));
        assert!(!owner.has_staged_companion());
        assert!(owner.application().is_none());
        assert!(owner.companion().is_none());
    }
}
