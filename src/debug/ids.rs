use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! debug_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            pub const UNSPECIFIED: Self = Self(0);
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

debug_id!(SessionId);
debug_id!(ContextId);
debug_id!(AddressSpaceId);
debug_id!(ProviderId);
debug_id!(OperationId);
debug_id!(CaptureId);
debug_id!(BreakpointId);
debug_id!(WatchpointId);
debug_id!(ArtifactId);
debug_id!(DecodeId);

#[derive(Debug, Default)]
pub(crate) struct IdAllocator {
    next: u64,
}

impl IdAllocator {
    pub(crate) fn new() -> Self {
        Self { next: 1 }
    }

    pub(crate) fn alloc(&mut self) -> u64 {
        let value = self.next;
        self.next = self.next.saturating_add(1);
        value
    }
}

#[derive(Debug)]
pub(crate) struct TypedAllocator<Id> {
    allocator: IdAllocator,
    _marker: std::marker::PhantomData<Id>,
}

impl<Id: DebugId> TypedAllocator<Id> {
    pub(crate) fn new() -> Self {
        Self {
            allocator: IdAllocator::new(),
            _marker: std::marker::PhantomData,
        }
    }

    pub(crate) fn alloc(&mut self) -> Id {
        Id::from_raw(self.allocator.alloc())
    }
}

pub(crate) trait DebugId: Copy {
    fn from_raw(raw: u64) -> Self;
}

macro_rules! impl_debug_id {
    ($($name:ident),* $(,)?) => {
        $(
            impl DebugId for $name {
                fn from_raw(raw: u64) -> Self {
                    Self(raw)
                }
            }
        )*
    };
}

impl_debug_id!(
    SessionId,
    ContextId,
    AddressSpaceId,
    ProviderId,
    OperationId,
    CaptureId,
    BreakpointId,
    WatchpointId,
    ArtifactId,
    DecodeId,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_allocators_mint_distinct_monotonic_ids() {
        let mut contexts = TypedAllocator::<ContextId>::new();
        assert_eq!(contexts.alloc(), ContextId(1));
        assert_eq!(contexts.alloc(), ContextId(2));

        let mut ops = TypedAllocator::<OperationId>::new();
        assert_eq!(ops.alloc(), OperationId(1));
    }

    #[test]
    fn display_includes_type_name() {
        assert_eq!(ContextId(7).to_string(), "ContextId#7");
    }
}
