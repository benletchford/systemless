//! Native engine lifetime ownership, independent of host presentation.
//!
//! A checked-out context retains its owner and role. No store borrow survives
//! checkout, so the execution coordinator can suspend native work while a
//! classic callback runs without borrowing the engine bank recursively.

use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeEngineRole {
    Application,
    Companion,
}

enum NativeSlot<T> {
    Empty,
    Installed(T),
    CheckedOut,
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
    adapter: T,
}

impl<T> NativeContext<T> {
    pub(crate) fn adapter_mut(&mut self) -> &mut T {
        &mut self.adapter
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
            application: matches!(self.application, NativeSlot::Installed(_)),
            companion: matches!(self.companion, NativeSlot::Installed(_)),
            staged_companion: self.staged.is_some() && matches!(self.companion, NativeSlot::Empty),
        }
    }

    pub(crate) fn application(&self) -> Option<&T> {
        match &self.application {
            NativeSlot::Installed(adapter) => Some(adapter),
            _ => None,
        }
    }

    pub(crate) fn application_mut(&mut self) -> Option<&mut T> {
        match &mut self.application {
            NativeSlot::Installed(adapter) => Some(adapter),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn companion(&self) -> Option<&T> {
        match &self.companion {
            NativeSlot::Installed(adapter) => Some(adapter),
            _ => None,
        }
    }

    pub(crate) fn install(&mut self, role: NativeEngineRole, adapter: T) -> Result<(), T> {
        if !matches!(self.slot(role), NativeSlot::Empty) {
            return Err(adapter);
        }
        *self.slot_mut(role) = NativeSlot::Installed(adapter);
        Ok(())
    }

    pub(crate) fn take(&mut self, role: NativeEngineRole) -> Option<NativeContext<T>> {
        if !matches!(self.slot(role), NativeSlot::Installed(_)) {
            return None;
        }
        let NativeSlot::Installed(adapter) =
            std::mem::replace(self.slot_mut(role), NativeSlot::CheckedOut)
        else {
            unreachable!("installed slot was validated before checkout")
        };
        Some(NativeContext {
            identity: Rc::clone(&self.identity),
            role,
            adapter,
        })
    }

    pub(crate) fn restore(&mut self, context: NativeContext<T>) -> Result<(), NativeContext<T>> {
        if !Rc::ptr_eq(&self.identity, &context.identity)
            || !matches!(self.slot(context.role), NativeSlot::CheckedOut)
        {
            return Err(context);
        }
        *self.slot_mut(context.role) = NativeSlot::Installed(context.adapter);
        Ok(())
    }

    pub(crate) fn can_relaunch(&self) -> bool {
        !matches!(self.application, NativeSlot::CheckedOut)
            && !matches!(self.companion, NativeSlot::CheckedOut)
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
