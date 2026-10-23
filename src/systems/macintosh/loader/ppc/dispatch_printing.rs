use super::*;
use crate::printing_manager::{self, PrintingArchitecture};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcPrintingCompatibilityOperation {
    PrClose,
    PrCloseDoc,
    PrClosePage,
    PrError,
    PrGeneral,
    PrSetError,
    PrValidate,
    PrJobDialog,
    PrOpen,
    PrOpenDoc,
    PrOpenPage,
    PrPicFile,
    PrStlDialog,
    PrintDefault,
}

/// Color Picker Package GetColor, presented by the host color picker.
/// FUNCTION GetColor(where: Point; prompt: Str255; inColor: RGBColor;
/// VAR outColor: RGBColor): BOOLEAN; Inside Macintosh Volume V, V-174.
pub(crate) fn ppc_dispatch_get_color(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
) -> PpcImportAction {
    use crate::systems::macintosh::host_ui::{SystemDialogAnswer, SystemDialogKind};
    if startup.host_ui.system_dialog_active() {
        return match startup.host_ui.take_system_answer() {
            Some(SystemDialogAnswer::Color(rgb)) => {
                let out = cpu.gpr[6];
                if out != 0 {
                    for (index, component) in rgb.into_iter().enumerate() {
                        let _ = memory.write_u16_be(out + index as u32 * 2, component);
                    }
                }
                PpcImportAction::Return(1)
            }
            Some(_) => PpcImportAction::Return(0),
            None => PpcImportAction::Yield(u64::MAX),
        };
    }
    let input = cpu.gpr[5];
    let color = [0u32, 2, 4].map(|offset| {
        if input == 0 {
            0
        } else {
            memory.read_u16_be(input + offset).unwrap_or(0)
        }
    });
    let prompt = if cpu.gpr[4] == 0 {
        String::new()
    } else {
        let length = memory.read_u8(cpu.gpr[4]).unwrap_or(0);
        let bytes: Vec<u8> = (1..=u32::from(length))
            .filter_map(|offset| memory.read_u8(cpu.gpr[4] + offset))
            .collect();
        crate::trap::types::decode_mac_roman(&bytes)
    };
    if startup
        .host_ui
        .begin_system_dialog(SystemDialogKind::Color { prompt, color })
    {
        PpcImportAction::Yield(u64::MAX)
    } else {
        PpcImportAction::Return(0)
    }
}

pub(crate) fn ppc_dispatch_printing_compatibility(
    operation: PpcPrintingCompatibilityOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
) -> PpcImportAction {
    match operation {
        PpcPrintingCompatibilityOperation::PrSetError => {
            // PrSetError stores the current Printing Manager error.
            // PROCEDURE PrSetError(iErr: Integer);
            // Inside Macintosh: Imaging With QuickDraw (1994), p. 9-78.
            startup.printing_error =
                printing_manager::evaluate_pr_set_error(cpu.gpr[3] as u16 as i16);
            PpcImportAction::ReturnPreserve
        }
        PpcPrintingCompatibilityOperation::PrGeneral => {
            // PrGeneral reports an unsupported printer-driver opcode in
            // TGnlData.iError and in the Printing Manager's error state.
            // PROCEDURE PrGeneral(pData: Ptr);
            // Inside Macintosh: Imaging With QuickDraw (1994), pp. 9-72--9-73.
            let data = cpu.gpr[3];
            let memory_writable = data != 0
                && memory.read_u16_be(data).is_some()
                && memory.write_u16_be(data + 2, 2).is_some();
            if let Some(err) = printing_manager::evaluate_pr_general(memory_writable) {
                startup.printing_error = err;
            }
            PpcImportAction::ReturnPreserve
        }
        PpcPrintingCompatibilityOperation::PrJobDialog
        | PpcPrintingCompatibilityOperation::PrStlDialog => {
            use crate::systems::macintosh::host_ui::{SystemDialogAnswer, SystemDialogKind};
            // With a host UI the import stays parked on the egui dialog.
            if startup.host_ui.system_dialog_active() {
                return match startup.host_ui.take_system_answer() {
                    Some(answer) => {
                        PpcImportAction::Return(u32::from(answer == SystemDialogAnswer::Confirm))
                    }
                    None => PpcImportAction::Yield(u64::MAX),
                };
            }
            let kind = if operation == PpcPrintingCompatibilityOperation::PrJobDialog {
                SystemDialogKind::Print
            } else {
                SystemDialogKind::PageSetup
            };
            if startup.host_ui.begin_system_dialog(kind) {
                return PpcImportAction::Yield(u64::MAX);
            }
            let confirmed = printing_manager::evaluate_pr_dialog(PrintingArchitecture::PowerPc);
            PpcImportAction::Return(u32::from(confirmed))
        }
        PpcPrintingCompatibilityOperation::PrOpenDoc => {
            PpcImportAction::Return(printing_manager::evaluate_pr_open_doc())
        }
        PpcPrintingCompatibilityOperation::PrError => {
            // FUNCTION PrError: Integer;
            // Inside Macintosh: Imaging With QuickDraw (1994), p. 9-75.
            PpcImportAction::Return(ppc_i16_result(printing_manager::evaluate_pr_error(
                startup.printing_error,
            )))
        }
        PpcPrintingCompatibilityOperation::PrValidate => {
            // PrValidate returns FALSE when the existing TPrint needs no
            // printer-specific changes. No printer driver is installed here.
            // FUNCTION PrValidate(hPrint: THPrint): Boolean;
            // Inside Macintosh: Imaging With QuickDraw (1994), p. 9-60.
            PpcImportAction::Return(u32::from(printing_manager::evaluate_pr_validate()))
        }
        PpcPrintingCompatibilityOperation::PrintDefault => {
            PpcImportAction::Return(printing_manager::evaluate_print_default())
        }
        PpcPrintingCompatibilityOperation::PrClose
        | PpcPrintingCompatibilityOperation::PrCloseDoc
        | PpcPrintingCompatibilityOperation::PrClosePage
        | PpcPrintingCompatibilityOperation::PrOpen
        | PpcPrintingCompatibilityOperation::PrOpenPage
        | PpcPrintingCompatibilityOperation::PrPicFile => PpcImportAction::ReturnPreserve,
    }
}

