//! Speech Recognition Manager entry points when no recognition engine exists.
use super::*;

// Universal Interfaces SpeechRecognition.h (2.2 and 3.4.1).
const NOT_AVAILABLE: i16 = -5100;
const NOT_A_SPEECH_OBJECT: i16 = -5104;
const BAD_PARAMETER: i16 = -5105;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcSpeechRecognitionOperation {
    Open,
    ObjectOutput,
    GetProperty,
    InvalidObject,
    Idle,
}

impl PpcSpeechRecognitionOperation {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "SROpenRecognitionSystem" => Self::Open,
            "SRNewRecognizer" | "SRNewLanguageModel" | "SRGetIndexedItem" => Self::ObjectOutput,
            "SRGetProperty" => Self::GetProperty,
            "SRCloseRecognitionSystem"
            | "SRContinueRecognition"
            | "SREmptyLanguageObject"
            | "SRReleaseObject"
            | "SRSetLanguageModel"
            | "SRSetProperty"
            | "SRStartListening"
            | "SRStopListening"
            | "SRAddText" => Self::InvalidObject,
            "SRIdle" => Self::Idle,
            _ => return None,
        })
    }
}

pub(super) fn dispatch(
    operation: PpcSpeechRecognitionOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    let result = match operation {
        PpcSpeechRecognitionOperation::Open | PpcSpeechRecognitionOperation::ObjectOutput => {
            let output = if operation == PpcSpeechRecognitionOperation::Open {
                cpu.gpr[3]
            } else {
                cpu.gpr[4]
            };
            if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                BAD_PARAMETER
            } else {
                let _ = memory.write_u32_be(output, 0);
                if operation == PpcSpeechRecognitionOperation::Open {
                    NOT_AVAILABLE
                } else {
                    // Opening never creates a system, so no supplied object
                    // reference can belong to this manager.
                    NOT_A_SPEECH_OBJECT
                }
            }
        }
        PpcSpeechRecognitionOperation::GetProperty => {
            // SRGetProperty(object, selector, buffer, inOutSize). No object
            // exists, so leave the buffer alone and return no property bytes.
            let size = cpu.gpr[6];
            if size == 0 || !ppc_memory_can_write_bytes(memory, size, 4) {
                BAD_PARAMETER
            } else {
                let _ = memory.write_u32_be(size, 0);
                NOT_A_SPEECH_OBJECT
            }
        }
        PpcSpeechRecognitionOperation::InvalidObject => NOT_A_SPEECH_OBJECT,
        PpcSpeechRecognitionOperation::Idle => PPC_NO_ERR,
    };
    PpcImportAction::Return(ppc_i16_result(result))
}
