use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcPrintingCompatibilityOperation {
    PrClose,
    PrCloseDoc,
    PrClosePage,
    PrError,
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
) -> PpcImportAction {
    match operation {
        PpcPrintingCompatibilityOperation::PrJobDialog
        | PpcPrintingCompatibilityOperation::PrStlDialog
        | PpcPrintingCompatibilityOperation::PrOpenDoc => PpcImportAction::Return(0),
        PpcPrintingCompatibilityOperation::PrError
        | PpcPrintingCompatibilityOperation::PrintDefault => {
            PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
        }
        PpcPrintingCompatibilityOperation::PrClose
        | PpcPrintingCompatibilityOperation::PrCloseDoc
        | PpcPrintingCompatibilityOperation::PrClosePage
        | PpcPrintingCompatibilityOperation::PrOpen
        | PpcPrintingCompatibilityOperation::PrOpenPage
        | PpcPrintingCompatibilityOperation::PrPicFile => PpcImportAction::ReturnPreserve,
    }
}
