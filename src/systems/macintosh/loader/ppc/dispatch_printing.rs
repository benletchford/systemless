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

