use super::ids::{AddressSpaceId, CaptureId, ContextId, OperationId};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum DebugError {
    Unsupported {
        operation: String,
    },
    InvalidState {
        detail: String,
    },
    StaleReference {
        detail: String,
    },
    InvalidValue {
        detail: String,
    },
    InvalidRange {
        detail: String,
    },
    Inaccessible {
        space: AddressSpaceId,
        detail: String,
    },
    TooLarge {
        limit: u64,
        requested: u64,
    },
    BoundaryUnavailable {
        detail: String,
    },
    CaptureExpired {
        id: CaptureId,
    },
    UnknownContext {
        id: ContextId,
    },
    UnknownOperation {
        id: OperationId,
    },
    Internal {
        detail: String,
    },
}

impl DebugError {
    pub fn unsupported(operation: impl Into<String>) -> Self {
        DebugError::Unsupported {
            operation: operation.into(),
        }
    }

    pub fn invalid_state(detail: impl Into<String>) -> Self {
        DebugError::InvalidState {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for DebugError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DebugError::Unsupported { operation } => {
                write!(f, "unsupported debug operation: {operation}")
            }
            DebugError::InvalidState { detail } => write!(f, "invalid debugger state: {detail}"),
            DebugError::StaleReference { detail } => {
                write!(f, "stale debugger reference: {detail}")
            }
            DebugError::InvalidValue { detail } => write!(f, "invalid debug value: {detail}"),
            DebugError::InvalidRange { detail } => write!(f, "invalid debug range: {detail}"),
            DebugError::Inaccessible { space, detail } => {
                write!(f, "inaccessible address space {space}: {detail}")
            }
            DebugError::TooLarge { limit, requested } => {
                write!(f, "request too large: limit {limit}, requested {requested}")
            }
            DebugError::BoundaryUnavailable { detail } => {
                write!(f, "capture boundary unavailable: {detail}")
            }
            DebugError::CaptureExpired { id } => write!(f, "capture {id} has expired"),
            DebugError::UnknownContext { id } => write!(f, "unknown context {id}"),
            DebugError::UnknownOperation { id } => write!(f, "unknown operation {id}"),
            DebugError::Internal { detail } => write!(f, "internal debugger failure: {detail}"),
        }
    }
}

impl std::error::Error for DebugError {}

pub type DebugResult<T> = Result<T, DebugError>;
