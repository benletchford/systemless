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
    PrDlgMain,
    PrJobInit,
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
    if !startup.host_ui.enabled {
        let out = cpu.gpr[6];
        if out != 0 {
            for (index, component) in color.into_iter().enumerate() {
                let _ = memory.write_u16_be(out + index as u32 * 2, component);
            }
        }
        return PpcImportAction::Return(0);
    }
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
        // Inside Macintosh: Imaging With QuickDraw (PrJobInit, PrDlgMain):
        // with no printer driver there is no job dialog to initialize, so
        // PrJobInit returns nil and PrDlgMain ends the way PrJobDialog does.
        PpcPrintingCompatibilityOperation::PrJobInit => PpcImportAction::Return(0),
        PpcPrintingCompatibilityOperation::PrJobDialog
        | PpcPrintingCompatibilityOperation::PrDlgMain
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

// PMDefinitions.h result codes.
const PPC_PM_OUT_OF_SCOPE: i16 = -30871;
const PPC_PM_NO_DEFAULT_PRINTER: i16 = -30872;
const PPC_PM_INVALID_PRINT_SETTINGS: i16 = -30875;
const PPC_PM_INVALID_PAGE_FORMAT: i16 = -30876;

// The page format and print settings objects are opaque to applications.
// Each is a tagged block so that a reference can be validated, and its
// flattened form is a copy of the block.
const PPC_PM_PAGE_FORMAT_TAG: u32 = u32::from_be_bytes(*b"PMpf");
const PPC_PM_PRINT_SETTINGS_TAG: u32 = u32::from_be_bytes(*b"PMps");
const PPC_PM_PAGE_FORMAT_SIZE: u32 = 20;
const PPC_PM_PRINT_SETTINGS_SIZE: u32 = 16;
// With no printer driver to describe a page, the defaults describe a US
// Letter sheet at 72 dpi with quarter-inch margins: the paper rectangle is
// relative to the imageable page rectangle's origin.
const PPC_PM_DEFAULT_PAPER_RECT: [i16; 4] = [-18, -18, 774, 594];
const PPC_PM_DEFAULT_PAGE_RECT: [i16; 4] = [0, 0, 756, 576];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcCarbonPrintingOperation {
    Begin,
    End,
    NewPageFormat,
    NewPrintSettings,
    DefaultPageFormat,
    DefaultPrintSettings,
    ValidatePageFormat,
    ValidatePrintSettings,
    DisposePageFormat,
    DisposePrintSettings,
    FlattenPageFormat,
    FlattenPrintSettings,
    UnflattenPageFormat,
    UnflattenPrintSettings,
    GetAdjustedPageRect,
    PageSetupDialog,
    PrintDialog,
    BeginDocument,
    EndDocument,
    BeginPage,
    EndPage,
}

impl PpcCarbonPrintingOperation {
    /// Whether the object is a page format rather than print settings.
    fn page_format(self) -> bool {
        matches!(
            self,
            Self::NewPageFormat
                | Self::DefaultPageFormat
                | Self::ValidatePageFormat
                | Self::DisposePageFormat
                | Self::FlattenPageFormat
                | Self::UnflattenPageFormat
        )
    }
}

fn ppc_pm_object_spec(page_format: bool) -> (u32, u32, i16) {
    if page_format {
        (
            PPC_PM_PAGE_FORMAT_TAG,
            PPC_PM_PAGE_FORMAT_SIZE,
            PPC_PM_INVALID_PAGE_FORMAT,
        )
    } else {
        (
            PPC_PM_PRINT_SETTINGS_TAG,
            PPC_PM_PRINT_SETTINGS_SIZE,
            PPC_PM_INVALID_PRINT_SETTINGS,
        )
    }
}

fn ppc_pm_default_object(page_format: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if page_format {
        bytes.extend_from_slice(&PPC_PM_PAGE_FORMAT_TAG.to_be_bytes());
        for value in PPC_PM_DEFAULT_PAPER_RECT
            .iter()
            .chain(&PPC_PM_DEFAULT_PAGE_RECT)
        {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    } else {
        // First page 1, last page "all pages", one copy.
        bytes.extend_from_slice(&PPC_PM_PRINT_SETTINGS_TAG.to_be_bytes());
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&u32::MAX.to_be_bytes());
        bytes.extend_from_slice(&1_u32.to_be_bytes());
    }
    bytes
}

fn ppc_pm_is_object(memory: &mut PpcSectionMem, object: u32, tag: u32) -> bool {
    object != 0 && memory.read_u32_be(object) == Some(tag)
}

/// Allocates a tagged object holding `bytes` and stores it at `out`.
fn ppc_pm_new_object(
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    out: u32,
    bytes: &[u8],
) -> i16 {
    if out == 0 || !ppc_memory_can_write_bytes(memory, out, 4) {
        return PPC_PARAM_ERR;
    }
    let object = process_memory_manager.new_native_ptr(memory, bytes.len() as u32, true);
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
    let _ = memory.write_u32_be(out, object);
    if object == 0 {
        return PPC_MEM_FULL_ERR;
    }
    let _ = memory.write_bytes(object, bytes);
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_dispatch_carbon_printing(
    operation: PpcCarbonPrintingOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> PpcImportAction {
    // Carbon Printing Manager (PMApplication.h). The runtime has no printer
    // configured: the objects work, the dialogs are dismissed without
    // being accepted, and a document cannot be opened.
    let in_scope = startup.carbon_printing_depth > 0;
    let page_format = operation.page_format();
    let (tag, size, invalid) = ppc_pm_object_spec(page_format);
    let result = match operation {
        PpcCarbonPrintingOperation::Begin => {
            startup.carbon_printing_depth = startup.carbon_printing_depth.saturating_add(1);
            PPC_NO_ERR
        }
        PpcCarbonPrintingOperation::End => {
            if in_scope {
                startup.carbon_printing_depth -= 1;
                PPC_NO_ERR
            } else {
                PPC_PM_OUT_OF_SCOPE
            }
        }
        PpcCarbonPrintingOperation::NewPageFormat
        | PpcCarbonPrintingOperation::NewPrintSettings => {
            // PMNewPageFormat(PMPageFormat *) / PMNewPrintSettings(
            // PMPrintSettings *) create objects the caller later fills with
            // PMDefaultPageFormat or PMDefaultPrintSettings.
            if !in_scope {
                PPC_PM_OUT_OF_SCOPE
            } else {
                let bytes = ppc_pm_default_object(page_format);
                ppc_pm_new_object(
                    memory,
                    process_memory_manager,
                    heap_cursor,
                    last_mem_error,
                    cpu.gpr[3],
                    &bytes,
                )
            }
        }
        PpcCarbonPrintingOperation::DefaultPageFormat
        | PpcCarbonPrintingOperation::DefaultPrintSettings => {
            if !in_scope {
                PPC_PM_OUT_OF_SCOPE
            } else if !ppc_pm_is_object(memory, cpu.gpr[3], tag) {
                invalid
            } else {
                let _ = memory.write_bytes(cpu.gpr[3], &ppc_pm_default_object(page_format));
                PPC_NO_ERR
            }
        }
        PpcCarbonPrintingOperation::ValidatePageFormat
        | PpcCarbonPrintingOperation::ValidatePrintSettings => {
            // PMValidatePageFormat(format, Boolean *result) reports whether
            // validation changed the object. Without a printer driver
            // nothing constrains it, so nothing changes.
            if !ppc_pm_is_object(memory, cpu.gpr[3], tag) {
                invalid
            } else if cpu.gpr[4] != 0 && memory.write_u8(cpu.gpr[4], 0).is_none() {
                PPC_PARAM_ERR
            } else {
                PPC_NO_ERR
            }
        }
        PpcCarbonPrintingOperation::DisposePageFormat
        | PpcCarbonPrintingOperation::DisposePrintSettings => {
            if !ppc_pm_is_object(memory, cpu.gpr[3], tag) {
                invalid
            } else {
                let _ = memory.write_u32_be(cpu.gpr[3], 0);
                let _ = process_memory_manager.dispose_native_ptr(cpu.gpr[3]);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                PPC_NO_ERR
            }
        }
        PpcCarbonPrintingOperation::FlattenPageFormat
        | PpcCarbonPrintingOperation::FlattenPrintSettings => {
            // PMFlattenPageFormat(format, Handle *flatFormat) returns a new
            // handle the caller owns.
            let out = cpu.gpr[4];
            if !ppc_pm_is_object(memory, cpu.gpr[3], tag) {
                invalid
            } else if out == 0 || !ppc_memory_can_write_bytes(memory, out, 4) {
                PPC_PARAM_ERR
            } else {
                let bytes = ppc_memory_read_bytes(memory, cpu.gpr[3], size).unwrap_or_default();
                let handle = process_memory_manager.copy_bytes_to_new_native_handle(memory, &bytes);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                ppc_apply_process_native_handle(process_memory_manager, handles, handle);
                let _ = memory.write_u32_be(out, handle);
                if handle == 0 {
                    PPC_MEM_FULL_ERR
                } else {
                    PPC_NO_ERR
                }
            }
        }
        PpcCarbonPrintingOperation::UnflattenPageFormat
        | PpcCarbonPrintingOperation::UnflattenPrintSettings => {
            // PMUnflattenPageFormat(Handle flatFormat, PMPageFormat *)
            // rebuilds an object from a handle PMFlattenPageFormat made.
            let handle = cpu.gpr[3];
            let data = memory
                .read_u32_be(handle)
                .filter(|_| handle != 0)
                .unwrap_or(0);
            let length =
                process_memory_manager.process_handle_size_from_master_pointer(handle, data);
            let bytes = (length == Some(size))
                .then(|| ppc_memory_read_bytes(memory, data, size))
                .flatten()
                .filter(|bytes| bytes[..4] == tag.to_be_bytes());
            match bytes {
                Some(bytes) => ppc_pm_new_object(
                    memory,
                    process_memory_manager,
                    heap_cursor,
                    last_mem_error,
                    cpu.gpr[4],
                    &bytes,
                ),
                None => invalid,
            }
        }
        PpcCarbonPrintingOperation::GetAdjustedPageRect => {
            // PMGetAdjustedPageRect(format, PMRect *) stores the imageable
            // page rectangle as four doubles: top, left, bottom, right.
            let (format, out) = (cpu.gpr[3], cpu.gpr[4]);
            if !ppc_pm_is_object(memory, format, PPC_PM_PAGE_FORMAT_TAG) {
                PPC_PM_INVALID_PAGE_FORMAT
            } else if out == 0 || !ppc_memory_can_write_bytes(memory, out, 32) {
                PPC_PARAM_ERR
            } else {
                for index in 0..4 {
                    let value = memory.read_u16_be(format + 12 + 2 * index).unwrap_or(0) as i16;
                    let _ = memory.write_u64_be(out + 8 * index, f64::from(value).to_bits());
                }
                PPC_NO_ERR
            }
        }
        PpcCarbonPrintingOperation::PageSetupDialog | PpcCarbonPrintingOperation::PrintDialog => {
            // PMPageSetupDialog(format, Boolean *accepted) and
            // PMPrintDialog(settings, format, Boolean *accepted). No printer
            // is configured, so no dialog can be accepted; as with
            // PrStlDialog and PrJobDialog, the result is false.
            let accepted = if operation == PpcCarbonPrintingOperation::PageSetupDialog {
                cpu.gpr[4]
            } else {
                cpu.gpr[5]
            };
            if accepted != 0 {
                let _ = memory.write_u8(accepted, 0);
            }
            if in_scope {
                PPC_NO_ERR
            } else {
                PPC_PM_OUT_OF_SCOPE
            }
        }
        PpcCarbonPrintingOperation::BeginDocument => {
            // PMBeginDocument(settings, format, PMPrintContext *) needs a
            // printer to open a print job.
            if cpu.gpr[5] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[5], 0);
            }
            if in_scope {
                PPC_PM_NO_DEFAULT_PRINTER
            } else {
                PPC_PM_OUT_OF_SCOPE
            }
        }
        // No document can be open, so page and document ends are outside
        // the scope of any PMBeginDocument.
        PpcCarbonPrintingOperation::EndDocument
        | PpcCarbonPrintingOperation::BeginPage
        | PpcCarbonPrintingOperation::EndPage => PPC_PM_OUT_OF_SCOPE,
    };
    PpcImportAction::Return(ppc_i16_result(result))
}

