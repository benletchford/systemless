use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcAppleTalkCompatibilityOperation {
    GetBridgeAddress,
    GetNodeAddress,
    GetZoneList,
    MppOpen,
    NbpExtract,
    NbpSetEntity,
    NbpSetNte,
    PCloseSkt,
    PKillNbp,
    PLookupName,
    POpenSkt,
    PRegisterName,
    PRemoveName,
    PSetSelfSend,
    PWriteDdp,
    StandardNbp,
}

pub(crate) fn ppc_dispatch_appletalk_compatibility(
    operation: PpcAppleTalkCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    match operation {
        PpcAppleTalkCompatibilityOperation::NbpSetEntity => {
            let mut offset = 0u32;
            for source in [cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]] {
                let bytes = ppc_read_pstring_bytes(memory, source).unwrap_or_default();
                let bytes = &bytes[..bytes.len().min(32)];
                let _ = memory.write_u8(cpu.gpr[3] + offset, bytes.len() as u8);
                let _ = memory.write_bytes(cpu.gpr[3] + offset + 1, bytes);
                offset = offset.saturating_add(33);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcAppleTalkCompatibilityOperation::GetNodeAddress => {
            let _ = memory.write_u8(cpu.gpr[3], 0);
            let _ = memory.write_u16_be(cpu.gpr[4], 0);
            PpcImportAction::Return(ppc_i16_result(PPC_NO_MPP_ERR))
        }
        PpcAppleTalkCompatibilityOperation::GetBridgeAddress => PpcImportAction::Return(0),
        PpcAppleTalkCompatibilityOperation::GetZoneList
        | PpcAppleTalkCompatibilityOperation::MppOpen
        | PpcAppleTalkCompatibilityOperation::NbpExtract
        | PpcAppleTalkCompatibilityOperation::NbpSetNte
        | PpcAppleTalkCompatibilityOperation::PCloseSkt
        | PpcAppleTalkCompatibilityOperation::PKillNbp
        | PpcAppleTalkCompatibilityOperation::PLookupName
        | PpcAppleTalkCompatibilityOperation::POpenSkt
        | PpcAppleTalkCompatibilityOperation::PRegisterName
        | PpcAppleTalkCompatibilityOperation::PRemoveName
        | PpcAppleTalkCompatibilityOperation::PSetSelfSend
        | PpcAppleTalkCompatibilityOperation::PWriteDdp
        | PpcAppleTalkCompatibilityOperation::StandardNbp => {
            if cpu.gpr[3] != 0 {
                let _ = memory.write_u16_be(cpu.gpr[3] + 16, PPC_NO_MPP_ERR as u16);
            }
            PpcImportAction::Return(ppc_i16_result(PPC_NO_MPP_ERR))
        }
    }
}
