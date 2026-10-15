//! Architecture adapters only: classic stack results and CFM procedure pointers.
//! TickCount: Inside Macintosh: Macintosh Toolbox Essentials (1992), Event
//! Manager. PPC transition vectors: Mac OS Runtime Architectures (1997), ch. 2.
use crate::{
    cpu::{Call, Cpu},
    memory::{GuestPtr, Memory},
    Error, Result,
};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Gestalt,
    StillDown,
    FindWindow,
    PtInRect,
    GetMouse,
    Random,
    TETextBox,
    FrameRect,
    EraseRect,
    InvertRect,
    FillRect,
    LineTo,
    Line,
    CopyBits,
    GetNewWindow,
    SizeWindow,
    MoveWindow,
    SelectWindow,
    InvalRect,
    ValidRect,
    GetWindowReference,
    SetWindowReference,
    ShowHide,
    NewControl,
    ControlDraw,
    ControlDrawAll,
    ControlDispose,
    ControlKill,
    ControlVisible(bool),
    ControlHilite,
    ControlValue,
    ControlMin,
    ControlMax,
    ControlSetValue,
    ControlReference,
    ControlSetReference,
    ControlTitle,
    ControlSetTitle,
    ControlFind,
    ControlTest,
    SetRect,
    OffsetRect,
    InsetRect,
    GetNextEvent,
    EventAvail,
    SysEnvirons,
    SystemTask,
    MaxApplZone,
    GetApplLimit,
    BlockMove,
    InitDialogs,
    ParamText,
    GetParamText,
    GetDAString,
    SetDialogFont,
    ErrorSound,
    Cursor(crate::cursor::Op),
    SetCursor,
    SetMenuItemText,
    SetItemCommand,
    SetItemMark,
    CheckItem,
    AppendResMenu,
    CountMenuItems,
    GetMenuHandle,
    GetMenu,
    GetNewMBar,
    SetMenuBar,
    Menu(crate::menus::Op),
    TENew(bool),
    TEUpdate,
    TESetText,
    TECalText,
    TESetSelect,
    TEDelete,
    TEKey,
    TEIdle,
    TEActivate(bool),
    TEInit,
    TEGetText,
    TEGetStyleHandle,
    TEDispose,
    TextDraw(bool),
    TextBuffer(bool),
    TextWidth(bool),
    MoveTo,
    InitFonts,
    FontInfo,
    TextState(crate::text::Field),
    NewWindow,
    Window(crate::windows::Op),
    PortColor(bool),
    RoundRect(bool),
    GetMemFragment,
    CloseConnection,
    FindSymbol,
    CountSymbols,
    GetIndSymbol,
    InitGraf,
    CombineRegions(crate::region_geometry::Boolean),
    Region(crate::regions::Op),
    Port(crate::quickdraw::PortOp),
    PenMode,
    PenSize,
    PortSize,
    MovePortTo,
    WaitNextEvent,
    PostEvent,
    GetOSEvent,
    OSEventAvail,
    FlushEvents,
    SetEventMask,
    GetEvQHdr,

    CExit,
    BreakPoint,
    SetJump,
    LongJump,
    TickCount,
    SetToolboxTrapAddress,
    GetOSTrapAddress,
    GetToolboxTrapAddress,
    NGetTrapAddress,
    ExitToShell,
    NewPointer(bool),
    PointerSize,
    SetPointerSize,
    DisposePointer,
    NewHandle,
    NewHandleClear,
    DisposeHandle,
    HLock,
    HUnlock,
    ReleaseResource,
    DetachResource,
    GetHandleSize,
    SetHandleSize,
    GetResource,
    GetPattern,
    GetCursor,
    GetPicture,
    DrawPicture,
    SetSoundVol,
    GetSoundVol,
    PBControlSync,
    StripAddress,
    MemError,
    ResError,
    CurResFile,
    GetNamedResource,
    Get1NamedResource,
    FileIo(crate::files::Op),
    FileIoWithAsync(crate::files::Op),
    FcbInfo,
    CatalogInfo,
    FileInfo(bool),
    CreateDirectory,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    None,
    StackLong(GuestPtr),
    StackByte(GuestPtr),
    StackWord(GuestPtr),
    PpcR3,
    M68kHandle,
    M68kA0,
    M68kGestalt,
    PpcGestalt(GuestPtr),
    M68kFileError,
    M68kOSEvent,
    M68kD0,
    M68kError,
}
pub struct Abi {
    pub fixture: bool,
    pub imports: BTreeMap<u32, Service>,
}
impl Abi {
    pub fn fixtures() -> Self {
        Self {
            fixture: true,
            imports: BTreeMap::new(),
        }
    }
    pub fn macintosh() -> Self {
        Self {
            fixture: false,
            imports: BTreeMap::new(),
        }
    }
    pub fn resolve(library: &[u8], symbol: &[u8]) -> Option<Service> {
        if library == b"StdCLib" {
            return match symbol {
                b"exit" => Some(Service::CExit),
                b"_BreakPoint" => Some(Service::BreakPoint),
                b"__setjmp" => Some(Service::SetJump),
                b"longjmp" => Some(Service::LongJump),
                _ => None,
            };
        }
        if library != b"InterfaceLib" {
            return None;
        }
        match symbol {
            b"GetMemFragment" => Some(Service::GetMemFragment),
            b"CloseConnection" => Some(Service::CloseConnection),
            b"FrameRoundRect" => Some(Service::RoundRect(true)),
            b"PaintRoundRect" => Some(Service::RoundRect(false)),
            b"ForeColor" => Some(Service::PortColor(false)),
            b"BackColor" => Some(Service::PortColor(true)),
            b"NewControl" => Some(Service::NewControl),
            b"Draw1Control" => Some(Service::ControlDraw),
            b"DrawControls" => Some(Service::ControlDrawAll),
            b"DisposeControl" => Some(Service::ControlDispose),
            b"KillControls" => Some(Service::ControlKill),
            b"ShowControl" => Some(Service::ControlVisible(true)),
            b"HideControl" => Some(Service::ControlVisible(false)),
            b"HiliteControl" => Some(Service::ControlHilite),
            b"GetCtlValue" | b"GetControlValue" => Some(Service::ControlValue),
            b"GetCtlMin" | b"GetControlMinimum" => Some(Service::ControlMin),
            b"GetCtlMax" | b"GetControlMaximum" => Some(Service::ControlMax),
            b"SetCtlValue" | b"SetControlValue" => Some(Service::ControlSetValue),
            b"GetCRefCon" | b"GetControlReference" => Some(Service::ControlReference),
            b"SetCRefCon" | b"SetControlReference" => Some(Service::ControlSetReference),
            b"GetCTitle" | b"GetControlTitle" => Some(Service::ControlTitle),
            b"SetCTitle" | b"SetControlTitle" => Some(Service::ControlSetTitle),
            b"FindControl" => Some(Service::ControlFind),
            b"TestControl" => Some(Service::ControlTest),
            b"GetWRefCon" => Some(Service::GetWindowReference),
            b"SetWRefCon" => Some(Service::SetWindowReference),
            b"InvalRect" => Some(Service::InvalRect),
            b"ValidRect" => Some(Service::ValidRect),
            b"MoveWindow" => Some(Service::MoveWindow),
            b"SelectWindow" => Some(Service::SelectWindow),
            b"SizeWindow" => Some(Service::SizeWindow),
            b"ShowHide" => Some(Service::ShowHide),
            b"GetNewWindow" => Some(Service::GetNewWindow),
            b"NewWindow" => Some(Service::NewWindow),
            b"ShowWindow" => Some(Service::Window(crate::windows::Op::Show)),
            b"HideWindow" => Some(Service::Window(crate::windows::Op::Hide)),
            b"InitWindows" => Some(Service::Window(crate::windows::Op::Init)),
            b"FrontWindow" => Some(Service::Window(crate::windows::Op::Front)),
            b"CloseWindow" => Some(Service::Window(crate::windows::Op::Close)),
            b"DisposeWindow" => Some(Service::Window(crate::windows::Op::Dispose)),
            b"TextFont" => Some(Service::TextState(crate::text::Field::Font)),
            b"TextFace" => Some(Service::TextState(crate::text::Field::Face)),
            b"TextMode" => Some(Service::TextState(crate::text::Field::Mode)),
            b"TextSize" => Some(Service::TextState(crate::text::Field::Size)),
            b"GetFontInfo" => Some(Service::FontInfo),
            b"InitFonts" => Some(Service::InitFonts),
            b"DrawText" => Some(Service::TextBuffer(false)),
            b"TextWidth" => Some(Service::TextBuffer(true)),
            b"DrawChar" => Some(Service::TextDraw(false)),
            b"DrawString" => Some(Service::TextDraw(true)),
            b"CharWidth" => Some(Service::TextWidth(false)),
            b"StringWidth" => Some(Service::TextWidth(true)),
            b"LineTo" => Some(Service::LineTo),
            b"Line" => Some(Service::Line),
            b"MoveTo" => Some(Service::MoveTo),
            b"TENew" => Some(Service::TENew(false)),
            b"TEStyleNew" => Some(Service::TENew(true)),
            b"TEUpdate" => Some(Service::TEUpdate),
            b"TESetText" => Some(Service::TESetText),
            b"TECalText" => Some(Service::TECalText),
            b"TESetSelect" => Some(Service::TESetSelect),
            b"TEDelete" => Some(Service::TEDelete),
            b"TEKey" => Some(Service::TEKey),
            b"TEIdle" => Some(Service::TEIdle),
            b"TEActivate" => Some(Service::TEActivate(true)),
            b"TEDeactivate" => Some(Service::TEActivate(false)),
            b"SetRect" => Some(Service::SetRect),
            b"OffsetRect" => Some(Service::OffsetRect),
            b"InsetRect" => Some(Service::InsetRect),
            b"GetNextEvent" => Some(Service::GetNextEvent),
            b"EventAvail" => Some(Service::EventAvail),
            b"BeginUpdate" => Some(Service::Window(crate::windows::Op::Begin)),
            b"EndUpdate" => Some(Service::Window(crate::windows::Op::End)),
            b"Gestalt" => Some(Service::Gestalt),
            b"StillDown" => Some(Service::StillDown),
            b"FindWindow" => Some(Service::FindWindow),
            b"PtInRect" => Some(Service::PtInRect),
            b"GetMouse" => Some(Service::GetMouse),
            b"SystemTask" => Some(Service::SystemTask),
            b"SysEnvirons" => Some(Service::SysEnvirons),
            b"MaxApplZone" => Some(Service::MaxApplZone),
            b"GetApplLimit" => Some(Service::GetApplLimit),
            b"BlockMove" | b"BlockMoveData" => Some(Service::BlockMove),
            b"InitDialogs" => Some(Service::InitDialogs),
            b"ParamText" => Some(Service::ParamText),
            b"LMGetDAStrings" => Some(Service::GetDAString),
            b"GetParamText" => Some(Service::GetParamText),
            b"SetDialogFont" | b"SetDAFont" => Some(Service::SetDialogFont),
            b"ErrorSound" => Some(Service::ErrorSound),
            b"InitCursor" => Some(Service::Cursor(crate::cursor::Op::Init)),
            b"HideCursor" => Some(Service::Cursor(crate::cursor::Op::Hide)),
            b"ShowCursor" => Some(Service::Cursor(crate::cursor::Op::Show)),
            b"ObscureCursor" => Some(Service::Cursor(crate::cursor::Op::Obscure)),
            b"SetCursor" => Some(Service::SetCursor),
            b"GetMHandle" | b"GetMenuHandle" => Some(Service::GetMenuHandle),
            b"SetItem" | b"SetMenuItemText" => Some(Service::SetMenuItemText),
            b"SetItemCmd" => Some(Service::SetItemCommand),
            b"SetItemMark" => Some(Service::SetItemMark),
            b"CheckItem" | b"CheckMenuItem" => Some(Service::CheckItem),
            b"AddResMenu" | b"AppendResMenu" => Some(Service::AppendResMenu),
            b"CountMItems" | b"CountMenuItems" => Some(Service::CountMenuItems),
            b"GetMenu" => Some(Service::GetMenu),
            b"GetNewMBar" => Some(Service::GetNewMBar),
            b"SetMenuBar" => Some(Service::SetMenuBar),
            b"InitMenus" => Some(Service::Menu(crate::menus::Op::Init)),
            b"DrawMenuBar" => Some(Service::Menu(crate::menus::Op::Draw)),
            b"ClearMenuBar" => Some(Service::Menu(crate::menus::Op::Clear)),
            b"GetMenuBar" => Some(Service::Menu(crate::menus::Op::Snapshot)),
            b"TETextBox" | b"TextBox" => Some(Service::TETextBox),
            b"TEInit" => Some(Service::TEInit),
            b"TEGetText" => Some(Service::TEGetText),
            b"TEGetStyleHandle" => Some(Service::TEGetStyleHandle),
            b"TEDispose" => Some(Service::TEDispose),
            b"Random" => Some(Service::Random),
            b"FindSymbol" => Some(Service::FindSymbol),
            b"CountSymbols" => Some(Service::CountSymbols),
            b"GetIndSymbol" => Some(Service::GetIndSymbol),
            b"RectInRgn" => Some(Service::Region(crate::regions::Op::Intersects)),
            b"PtInRgn" => Some(Service::Region(crate::regions::Op::Contains)),
            b"EqualRgn" => Some(Service::Region(crate::regions::Op::Equal)),
            b"EmptyRgn" => Some(Service::Region(crate::regions::Op::Empty)),
            b"NewRgn" => Some(Service::Region(crate::regions::Op::New)),
            b"DisposeRgn" => Some(Service::Region(crate::regions::Op::Dispose)),
            b"CopyRgn" => Some(Service::Region(crate::regions::Op::Copy)),
            b"RectRgn" => Some(Service::Region(crate::regions::Op::Rect)),
            b"GetClip" => Some(Service::Region(crate::regions::Op::GetClip)),
            b"SetClip" => Some(Service::Region(crate::regions::Op::SetClip)),
            b"SectRgn" => Some(Service::CombineRegions(
                crate::region_geometry::Boolean::Intersection,
            )),
            b"UnionRgn" => Some(Service::CombineRegions(
                crate::region_geometry::Boolean::Union,
            )),
            b"DiffRgn" => Some(Service::CombineRegions(
                crate::region_geometry::Boolean::Difference,
            )),
            b"XorRgn" => Some(Service::CombineRegions(
                crate::region_geometry::Boolean::Xor,
            )),
            b"InitGraf" => Some(Service::InitGraf),
            b"LocalToGlobal" => Some(Service::Port(crate::quickdraw::PortOp::LocalToGlobal)),
            b"GlobalToLocal" => Some(Service::Port(crate::quickdraw::PortOp::GlobalToLocal)),
            b"InitPort" => Some(Service::Port(crate::quickdraw::PortOp::Init)),
            b"ClosePort" => Some(Service::Port(crate::quickdraw::PortOp::Close)),
            b"OpenPort" => Some(Service::Port(crate::quickdraw::PortOp::Open)),
            b"SetPort" => Some(Service::Port(crate::quickdraw::PortOp::Set)),
            b"GetPort" => Some(Service::Port(crate::quickdraw::PortOp::Get)),
            b"SetPortBits" => Some(Service::Port(crate::quickdraw::PortOp::SetBits)),
            b"ClipRect" => Some(Service::Port(crate::quickdraw::PortOp::ClipRect)),
            b"FrameRect" => Some(Service::FrameRect),
            b"EraseRect" => Some(Service::EraseRect),
            b"InvertRect" => Some(Service::InvertRect),
            b"FillRect" => Some(Service::FillRect),
            b"PaintRect" => Some(Service::Port(crate::quickdraw::PortOp::PaintRect)),
            b"PenPat" => Some(Service::Port(crate::quickdraw::PortOp::PenPat)),
            b"MovePortTo" => Some(Service::MovePortTo),
            b"PortSize" => Some(Service::PortSize),
            b"PenSize" => Some(Service::PenSize),
            b"PenMode" => Some(Service::PenMode),
            b"WaitNextEvent" => Some(Service::WaitNextEvent),
            b"PostEvent" => Some(Service::PostEvent),
            b"GetOSEvent" => Some(Service::GetOSEvent),
            b"OSEventAvail" => Some(Service::OSEventAvail),
            b"FlushEvents" => Some(Service::FlushEvents),
            b"SetEventMask" => Some(Service::SetEventMask),
            b"GetEvQHdr" => Some(Service::GetEvQHdr),
            b"SetToolboxTrapAddress" | b"SetToolTrapAddress" => {
                Some(Service::SetToolboxTrapAddress)
            }
            b"GetOSTrapAddress" => Some(Service::GetOSTrapAddress),
            b"GetToolboxTrapAddress" | b"GetToolTrapAddress" => {
                Some(Service::GetToolboxTrapAddress)
            }
            b"NGetTrapAddress" => Some(Service::NGetTrapAddress),
            b"TickCount" => Some(Service::TickCount),
            b"ExitToShell" => Some(Service::ExitToShell),
            b"NewPtr" => Some(Service::NewPointer(false)),
            b"NewPtrClear" => Some(Service::NewPointer(true)),
            b"GetPtrSize" => Some(Service::PointerSize),
            b"SetPtrSize" => Some(Service::SetPointerSize),
            b"DisposePtr" => Some(Service::DisposePointer),
            b"NewHandle" => Some(Service::NewHandle),
            b"NewHandleClear" => Some(Service::NewHandleClear),
            b"DisposeHandle" => Some(Service::DisposeHandle),
            b"SetHandleSize" => Some(Service::SetHandleSize),
            b"GetHandleSize" => Some(Service::GetHandleSize),
            b"ReleaseResource" => Some(Service::ReleaseResource),
            b"DetachResource" => Some(Service::DetachResource),
            b"SetSoundVol" => Some(Service::SetSoundVol),
            b"GetSoundVol" => Some(Service::GetSoundVol),
            b"PBControlSync" => Some(Service::PBControlSync),
            b"StripAddress" => Some(Service::StripAddress),
            b"HLock" => Some(Service::HLock),
            b"HUnlock" => Some(Service::HUnlock),
            b"CopyBits" => Some(Service::CopyBits),
            b"DrawPicture" => Some(Service::DrawPicture),
            b"GetPicture" => Some(Service::GetPicture),
            b"GetCursor" => Some(Service::GetCursor),
            b"GetPattern" => Some(Service::GetPattern),
            b"GetResource" => Some(Service::GetResource),
            b"MemError" => Some(Service::MemError),
            b"PBOpenSync" => Some(Service::FileIo(crate::files::Op::Open {
                kind: crate::files::Fork::Data,
                hierarchical: false,
            })),
            b"PBHOpenSync" | b"PBHOpenDFSync" => Some(Service::FileIo(crate::files::Op::Open {
                kind: crate::files::Fork::Data,
                hierarchical: true,
            })),
            b"PBOpenRFSync" => Some(Service::FileIo(crate::files::Op::Open {
                kind: crate::files::Fork::Resource,
                hierarchical: false,
            })),
            b"PBHOpenRFSync" => Some(Service::FileIo(crate::files::Op::Open {
                kind: crate::files::Fork::Resource,
                hierarchical: true,
            })),
            b"PBSetFInfoSync" => Some(Service::FileIo(crate::files::Op::SetInfo {
                hierarchical: false,
            })),
            b"PBHSetFInfoSync" => Some(Service::FileIo(crate::files::Op::SetInfo {
                hierarchical: true,
            })),
            b"PBCreateSync" => Some(Service::FileIo(crate::files::Op::Create {
                hierarchical: false,
            })),
            b"PBHCreateSync" => Some(Service::FileIo(crate::files::Op::Create {
                hierarchical: true,
            })),
            b"PBCloseSync" => Some(Service::FileIo(crate::files::Op::Close)),
            b"PBRead" => Some(Service::FileIoWithAsync(crate::files::Op::Read)),
            b"PBWrite" => Some(Service::FileIoWithAsync(crate::files::Op::Write)),
            b"PBClose" => Some(Service::FileIoWithAsync(crate::files::Op::Close)),
            b"PBReadSync" => Some(Service::FileIo(crate::files::Op::Read)),
            b"PBWriteSync" => Some(Service::FileIo(crate::files::Op::Write)),
            b"PBGetEOFSync" => Some(Service::FileIo(crate::files::Op::GetEof)),
            b"PBSetEOFSync" => Some(Service::FileIo(crate::files::Op::SetEof)),
            b"PBGetFPosSync" => Some(Service::FileIo(crate::files::Op::GetPosition)),
            b"PBSetFPosSync" => Some(Service::FileIo(crate::files::Op::SetPosition)),
            b"PBFlushFileSync" => Some(Service::FileIo(crate::files::Op::Flush)),
            b"PBGetFCBInfoSync" => Some(Service::FcbInfo),
            b"PBGetCatInfoSync" => Some(Service::CatalogInfo),
            b"PBGetFInfoSync" => Some(Service::FileInfo(false)),
            b"PBHGetFInfoSync" => Some(Service::FileInfo(true)),
            b"PBDirCreateSync" => Some(Service::CreateDirectory),
            b"GetNamedResource" => Some(Service::GetNamedResource),
            b"Get1NamedResource" => Some(Service::Get1NamedResource),
            b"CurResFile" => Some(Service::CurResFile),
            b"ResError" => Some(Service::ResError),
            _ => None,
        }
    }
    pub fn trap(opcode: u16, c: &mut m68k::CpuCore, memory: &Memory) -> Result<(Call, Reply)> {
        let sp = c.dar[15];
        match opcode {
            0xa8b0 | 0xa8b1 => {
                let bytes = memory.read(GuestPtr(sp), 8)?;
                let height = i16::from_be_bytes(bytes[..2].try_into().unwrap());
                let width = i16::from_be_bytes(bytes[2..4].try_into().unwrap());
                let pointer = GuestPtr(u32::from_be_bytes(bytes[4..8].try_into().unwrap()));
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("QuickDraw stack overflow".into()))?;
                Ok((
                    Call::RoundRect {
                        pointer,
                        width,
                        height,
                        frame: opcode == 0xa8b0,
                    },
                    Reply::None,
                ))
            }
            0xa862 | 0xa863 => {
                let color = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("QuickDraw stack overflow".into()))?;
                Ok((
                    Call::PortColor {
                        background: opcode == 0xa863,
                        color,
                    },
                    Reply::None,
                ))
            }
            0xa917 | 0xa918 => {
                let size = if opcode == 0xa917 { 4 } else { 8 };
                let args = memory.read(GuestPtr(sp), size)?;
                let pointer = u32::from_be_bytes(args[size - 4..].try_into().unwrap());
                let value = if opcode == 0xa918 {
                    Some(u32::from_be_bytes(args[..4].try_into().unwrap()))
                } else {
                    None
                };
                let result = sp
                    .checked_add(size as u32)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                let reply = if value.is_none() {
                    memory.validate_write(GuestPtr(result), 4)?;
                    Reply::StackLong(GuestPtr(result))
                } else {
                    Reply::None
                };
                c.dar[15] = result;
                Ok((Call::WindowReference { pointer, value }, reply))
            }
            0xa928 | 0xa92a => {
                let rectangle = GuestPtr(memory.u32(GuestPtr(sp))?);
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                Ok((
                    Call::WindowInvalidation {
                        rectangle,
                        valid: opcode == 0xa92a,
                    },
                    Reply::None,
                ))
            }
            0xa91b | 0xa91f => {
                let size = if opcode == 0xa91b { 10 } else { 4 };
                let args = memory.read(GuestPtr(sp), size)?;
                let pointer = u32::from_be_bytes(args[size - 4..].try_into().unwrap());
                let change = if opcode == 0xa91b {
                    crate::windows::Change::Move {
                        horizontal: i16::from_be_bytes(args[4..6].try_into().unwrap()),
                        vertical: i16::from_be_bytes(args[2..4].try_into().unwrap()),
                        front: args[0] != 0,
                    }
                } else {
                    crate::windows::Change::Select
                };
                c.dar[15] = sp
                    .checked_add(size as u32)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                Ok((Call::WindowChange { pointer, change }, Reply::None))
            }
            0xa91d | 0xa908 => {
                let size = if opcode == 0xa91d { 10 } else { 6 };
                let bytes = memory.read(GuestPtr(sp), size)?;
                let pointer = u32::from_be_bytes(bytes[size - 4..].try_into().unwrap());
                let change = if opcode == 0xa91d {
                    crate::windows::Change::Size {
                        width: i16::from_be_bytes(bytes[4..6].try_into().unwrap()),
                        height: i16::from_be_bytes(bytes[2..4].try_into().unwrap()),
                        update: bytes[0] != 0,
                    }
                } else {
                    crate::windows::Change::Visibility(bytes[0] != 0)
                };
                c.dar[15] = sp
                    .checked_add(size as u32)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                Ok((Call::WindowChange { pointer, change }, Reply::None))
            }
            0xa954 => {
                let bytes = memory.read(GuestPtr(sp), 26)?;
                let long = |o| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
                let word = |o| i16::from_be_bytes(bytes[o..o + 2].try_into().unwrap());
                let result = sp
                    .checked_add(26)
                    .ok_or_else(|| Error::Cpu("control stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                let request = crate::controls::NewControl {
                    owner: GuestPtr(long(22)),
                    bounds: GuestPtr(long(18)),
                    title: GuestPtr(long(14)),
                    visible: bytes[12] != 0,
                    value: word(10),
                    min: word(8),
                    max: word(6),
                    proc_id: word(4),
                    ref_con: long(0),
                };
                c.dar[15] = result;
                Ok((
                    Call::NewControl(request),
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa96c => {
                let args = memory.read(GuestPtr(sp), 12)?;
                let destination = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let owner = u32::from_be_bytes(args[4..8].try_into().unwrap());
                let point = [
                    i16::from_be_bytes(args[8..10].try_into().unwrap()),
                    i16::from_be_bytes(args[10..12].try_into().unwrap()),
                ];
                let result = sp
                    .checked_add(12)
                    .ok_or_else(|| Error::Cpu("FindControl stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::Control(crate::controls::Op::Find {
                        owner,
                        point,
                        destination,
                    }),
                    Reply::StackWord(GuestPtr(result)),
                ))
            }
            0xa955..=0xa958 | 0xa95a | 0xa960..=0xa962 | 0xa969 | 0xa96d => {
                use crate::controls::Op;
                let h = memory.u32(GuestPtr(sp))?;
                let result = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("control stack overflow".into()))?;
                let op = match opcode {
                    0xa955 => Op::Dispose(h),
                    0xa956 => Op::Kill(h),
                    0xa957 => Op::Visible(h, true),
                    0xa958 => Op::Visible(h, false),
                    0xa95a => Op::Reference(h),
                    0xa960 => Op::Value(h),
                    0xa961 => Op::Min(h),
                    0xa962 => Op::Max(h),
                    0xa969 => Op::DrawAll(h),
                    _ => Op::Draw(h),
                };
                let reply = if opcode == 0xa95a {
                    memory.validate_write(GuestPtr(result), 4)?;
                    Reply::StackLong(GuestPtr(result))
                } else if matches!(opcode, 0xa960..=0xa962) {
                    memory.validate_write(GuestPtr(result), 2)?;
                    Reply::StackWord(GuestPtr(result))
                } else {
                    Reply::None
                };
                c.dar[15] = result;
                Ok((Call::Control(op), reply))
            }
            0xa95b | 0xa95d..=0xa95f | 0xa963 | 0xa966 => {
                use crate::controls::Op;
                let size = if matches!(opcode, 0xa95d | 0xa963) {
                    6
                } else {
                    8
                };
                let bytes = memory.read(GuestPtr(sp), size)?;
                let h = u32::from_be_bytes(bytes[size - 4..].try_into().unwrap());
                let word = |o| i16::from_be_bytes(bytes[o..o + 2].try_into().unwrap());
                let value = if size == 8 {
                    u32::from_be_bytes(bytes[..4].try_into().unwrap())
                } else {
                    0
                };
                let op = match opcode {
                    0xa95b => Op::SetReference(h, value),
                    0xa95d => Op::Hilite(h, word(0) as u8),
                    0xa95e => Op::Title(h, GuestPtr(value)),
                    0xa95f => Op::SetTitle(h, GuestPtr(value)),
                    0xa963 => Op::SetValue(h, word(0)),
                    _ => Op::Test(h, word(2), word(0)),
                };
                let result = sp
                    .checked_add(size as u32)
                    .ok_or_else(|| Error::Cpu("control stack overflow".into()))?;
                let reply = if opcode == 0xa966 {
                    memory.validate_write(GuestPtr(result), 2)?;
                    Reply::StackWord(GuestPtr(result))
                } else {
                    Reply::None
                };
                c.dar[15] = result;
                Ok((Call::Control(op), reply))
            }
            0xa9bd => {
                let args = memory.read(GuestPtr(sp), 14)?;
                let result = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                let call = Call::ResourceWindow {
                    id: i16::from_be_bytes(args[8..10].try_into().unwrap()),
                    storage: GuestPtr(u32::from_be_bytes(args[4..8].try_into().unwrap())),
                    behind: u32::from_be_bytes(args[..4].try_into().unwrap()),
                };
                c.dar[15] = result;
                Ok((call, Reply::StackLong(GuestPtr(result))))
            }
            0xa913 => {
                let bytes = memory.read(GuestPtr(sp), 26)?;
                let long =
                    |offset| u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
                let result = sp
                    .checked_add(26)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                let request = crate::windows::NewWindow {
                    storage: GuestPtr(long(22)),
                    bounds: GuestPtr(long(18)),
                    title: GuestPtr(long(14)),
                    visible: bytes[12] != 0,
                    proc_id: i16::from_be_bytes(bytes[10..12].try_into().unwrap()),
                    behind: long(6),
                    go_away: bytes[4] != 0,
                    ref_con: long(0),
                };
                c.dar[15] = result;
                Ok((Call::NewWindow(request), Reply::StackLong(GuestPtr(result))))
            }
            0xa912 | 0xa924 => {
                let op = if opcode == 0xa912 {
                    crate::windows::Op::Init
                } else {
                    crate::windows::Op::Front
                };
                if opcode == 0xa924 {
                    memory.validate_write(GuestPtr(sp), 4)?;
                }
                Ok((
                    Call::Window { op, pointer: 0 },
                    if opcode == 0xa924 {
                        Reply::StackLong(GuestPtr(sp))
                    } else {
                        Reply::None
                    },
                ))
            }
            0xa92d | 0xa914 | 0xa922 | 0xa923 | 0xa915 | 0xa916 => {
                let pointer = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                Ok((
                    Call::Window {
                        op: if opcode == 0xa915 {
                            crate::windows::Op::Show
                        } else if opcode == 0xa916 {
                            crate::windows::Op::Hide
                        } else if opcode == 0xa922 {
                            crate::windows::Op::Begin
                        } else if opcode == 0xa923 {
                            crate::windows::Op::End
                        } else if opcode == 0xa92d {
                            crate::windows::Op::Close
                        } else {
                            crate::windows::Op::Dispose
                        },
                        pointer,
                    },
                    Reply::None,
                ))
            }
            0xa887..=0xa88a => {
                let word = memory.u16(GuestPtr(sp))?;
                let field = match opcode {
                    0xa887 => crate::text::Field::Font,
                    0xa888 => crate::text::Field::Face,
                    0xa889 => crate::text::Field::Mode,
                    _ => crate::text::Field::Size,
                };
                let value = word;
                c.dar[15] = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("text stack overflow".into()))?;
                Ok((Call::TextState { field, value }, Reply::None))
            }
            0xa88b => {
                let destination = GuestPtr(memory.u32(GuestPtr(sp))?);
                memory.validate_write(destination, 8)?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("font metrics stack overflow".into()))?;
                Ok((Call::FontInfo(destination), Reply::None))
            }
            0xa8fe => Ok((Call::InitFonts, Reply::None)),
            0xa891..=0xa893 => {
                let v = memory.u16(GuestPtr(sp))? as i16;
                let h = memory.u16(GuestPtr(
                    sp.checked_add(2)
                        .ok_or_else(|| Error::Cpu("point stack overflow".into()))?,
                ))? as i16;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("point stack overflow".into()))?;
                Ok((
                    if opcode == 0xa893 {
                        Call::MoveTo { h, v }
                    } else {
                        Call::PenLine {
                            h,
                            v,
                            relative: opcode == 0xa892,
                        }
                    },
                    Reply::None,
                ))
            }
            0xa885 | 0xa886 => {
                let bytes = memory.read(GuestPtr(sp), 8)?;
                let input = crate::text::Input::Buffer {
                    pointer: GuestPtr(u32::from_be_bytes(bytes[4..8].try_into().unwrap())),
                    first: i16::from_be_bytes(bytes[2..4].try_into().unwrap()),
                    count: i16::from_be_bytes(bytes[..2].try_into().unwrap()),
                };
                let result = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("text buffer stack overflow".into()))?;
                if opcode == 0xa886 {
                    memory.validate_write(GuestPtr(result), 2)?;
                }
                c.dar[15] = result;
                Ok((
                    if opcode == 0xa886 {
                        Call::TextWidth(input)
                    } else {
                        Call::TextDraw(input)
                    },
                    if opcode == 0xa886 {
                        Reply::StackWord(GuestPtr(result))
                    } else {
                        Reply::None
                    },
                ))
            }
            0xa883 | 0xa884 | 0xa88c | 0xa88d => {
                let string = matches!(opcode, 0xa884 | 0xa88c);
                let input = if string {
                    crate::text::Input::Pascal(GuestPtr(memory.u32(GuestPtr(sp))?))
                } else {
                    crate::text::Input::Character(memory.u16(GuestPtr(sp))? as u8)
                };
                let result = sp
                    .checked_add(if string { 4 } else { 2 })
                    .ok_or_else(|| Error::Cpu("text stack overflow".into()))?;
                let width = matches!(opcode, 0xa88c | 0xa88d);
                if width {
                    memory.validate_write(GuestPtr(result), 2)?;
                }
                c.dar[15] = result;
                Ok((
                    if width {
                        Call::TextWidth(input)
                    } else {
                        Call::TextDraw(input)
                    },
                    if width {
                        Reply::StackWord(GuestPtr(result))
                    } else {
                        Reply::None
                    },
                ))
            }
            0xa9d2 | 0xa83e => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let view = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let dest = GuestPtr(u32::from_be_bytes(args[4..].try_into().unwrap()));
                let result = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("TextEdit stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                c.dar[15] = result;
                Ok((
                    Call::TextEdit(crate::textedit::Op::New {
                        dest,
                        view,
                        styled: opcode == 0xa83e,
                    }),
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa9d3 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let handle = u32::from_be_bytes(args[..4].try_into().unwrap());
                let rect = GuestPtr(u32::from_be_bytes(args[4..8].try_into().unwrap()));
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("TextEdit stack overflow".into()))?;
                Ok((
                    Call::TextEdit(crate::textedit::Op::Update { rect, handle }),
                    Reply::None,
                ))
            }
            0xa9cf => {
                let args = memory.read(GuestPtr(sp), 12)?;
                let handle = u32::from_be_bytes(args[..4].try_into().unwrap());
                let length = u32::from_be_bytes(args[4..8].try_into().unwrap());
                let source = GuestPtr(u32::from_be_bytes(args[8..12].try_into().unwrap()));
                c.dar[15] = sp
                    .checked_add(12)
                    .ok_or_else(|| Error::Cpu("TextEdit stack overflow".into()))?;
                Ok((
                    Call::TextEdit(crate::textedit::Op::SetText {
                        source,
                        length,
                        handle,
                    }),
                    Reply::None,
                ))
            }
            0xa8e3 => {
                let args = memory.read(GuestPtr(sp), 10)?;
                let b = u32::from_be_bytes(args[..4].try_into().unwrap());
                let a = u32::from_be_bytes(args[4..8].try_into().unwrap());
                let result = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("region stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 1)?;
                c.dar[15] = result;
                Ok((
                    Call::Region {
                        op: crate::regions::Op::Equal,
                        a,
                        b,
                    },
                    Reply::StackByte(GuestPtr(result)),
                ))
            }
            0xa8a7..=0xa8a9 => {
                let size = if opcode == 0xa8a7 { 12 } else { 8 };
                let args = memory.read(GuestPtr(sp), size)?;
                let word = |i| i16::from_be_bytes(args[i..i + 2].try_into().unwrap());
                let pointer = GuestPtr(u32::from_be_bytes(args[size - 4..].try_into().unwrap()));
                memory.validate_write(pointer, 8)?;
                let op = match opcode {
                    0xa8a7 => crate::rectangles::Op::Set {
                        pointer,
                        edges: [word(4), word(6), word(0), word(2)],
                    },
                    0xa8a8 => crate::rectangles::Op::Offset {
                        pointer,
                        dh: word(2),
                        dv: word(0),
                    },
                    _ => crate::rectangles::Op::Inset {
                        pointer,
                        dh: word(2),
                        dv: word(0),
                    },
                };
                c.dar[15] = sp
                    .checked_add(size as u32)
                    .ok_or_else(|| Error::Cpu("rectangle stack overflow".into()))?;
                Ok((Call::Rectangle(op), Reply::None))
            }
            0xa970 | 0xa971 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let destination = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let mask = u16::from_be_bytes(args[4..6].try_into().unwrap());
                let result = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("event stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 1)?;
                c.dar[15] = result;
                Ok((
                    Call::ToolboxEvent {
                        mask,
                        destination,
                        remove: opcode == 0xa970,
                    },
                    Reply::StackByte(GuestPtr(result)),
                ))
            }
            0xa0ad | 0xa1ad => Ok((Call::Gestalt(c.dar[0]), Reply::M68kGestalt)),
            0xa973 => {
                memory.validate_write(GuestPtr(sp), 1)?;
                Ok((Call::StillDown, Reply::StackByte(GuestPtr(sp))))
            }
            0xa92c => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let destination = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let point = [
                    i16::from_be_bytes(args[4..6].try_into().unwrap()),
                    i16::from_be_bytes(args[6..8].try_into().unwrap()),
                ];
                let result = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("FindWindow stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::FindWindow { point, destination },
                    Reply::StackWord(GuestPtr(result)),
                ))
            }
            0xa8ad => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let rectangle = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let point = [
                    i16::from_be_bytes(args[4..6].try_into().unwrap()),
                    i16::from_be_bytes(args[6..8].try_into().unwrap()),
                ];
                let result = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("PtInRect stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 1)?;
                c.dar[15] = result;
                Ok((
                    Call::PointInRect { point, rectangle },
                    Reply::StackByte(GuestPtr(result)),
                ))
            }
            0xa972 => {
                let point = GuestPtr(memory.u32(GuestPtr(sp))?);
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("GetMouse stack overflow".into()))?;
                Ok((Call::GetMouse(point), Reply::None))
            }
            0xa9b4 => Ok((Call::SystemTask, Reply::None)),
            0xa090 => Ok((
                Call::SysEnvirons {
                    version: c.dar[0] as i16,
                    record: GuestPtr(c.dar[8]),
                },
                Reply::M68kD0,
            )),
            0xa063 => Ok((Call::MaxApplZone, Reply::M68kD0)),
            0xa02e | 0xa22e => Ok((
                Call::MoveBytes {
                    source: GuestPtr(c.dar[8]),
                    destination: GuestPtr(c.dar[9]),
                    count: c.dar[0] as i32,
                },
                Reply::M68kD0,
            )),
            0xa97b | 0xa98c => {
                let pointer = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("dialog stack overflow".into()))?;
                Ok((
                    Call::Dialog(if opcode == 0xa97b {
                        crate::dialogs::Op::Init(pointer)
                    } else {
                        crate::dialogs::Op::Sound(pointer)
                    }),
                    Reply::None,
                ))
            }
            0xa98b => {
                let args = memory.read(GuestPtr(sp), 16)?;
                let inputs = std::array::from_fn(|i| {
                    GuestPtr(u32::from_be_bytes(
                        args[12 - i * 4..16 - i * 4].try_into().unwrap(),
                    ))
                });
                c.dar[15] = sp
                    .checked_add(16)
                    .ok_or_else(|| Error::Cpu("dialog stack overflow".into()))?;
                Ok((Call::Dialog(crate::dialogs::Op::Param(inputs)), Reply::None))
            }
            0xa850 | 0xa852 | 0xa853 | 0xa856 => Ok((
                Call::Cursor(match opcode {
                    0xa850 => crate::cursor::Op::Init,
                    0xa852 => crate::cursor::Op::Hide,
                    0xa853 => crate::cursor::Op::Show,
                    _ => crate::cursor::Op::Obscure,
                }),
                Reply::None,
            )),
            0xa851 => {
                let pointer = GuestPtr(memory.u32(GuestPtr(sp))?);
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("cursor stack overflow".into()))?;
                Ok((Call::Cursor(crate::cursor::Op::Set(pointer)), Reply::None))
            }
            0xa930 | 0xa934 | 0xa937 => Ok((
                Call::Menu(match opcode {
                    0xa930 => crate::menus::Op::Init,
                    0xa934 => crate::menus::Op::Clear,
                    _ => crate::menus::Op::Draw,
                }),
                Reply::None,
            )),
            0xa947 => {
                let args = memory.read(GuestPtr(sp), 10)?;
                let source = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let item = i16::from_be_bytes(args[4..6].try_into().unwrap());
                let handle = u32::from_be_bytes(args[6..10].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("menu text stack overflow".into()))?;
                Ok((
                    Call::Menu(crate::menus::Op::Text {
                        handle,
                        item,
                        source,
                    }),
                    Reply::None,
                ))
            }
            0xa84f | 0xa944 | 0xa945 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let mark = if opcode == 0xa945 {
                    if args[0] != 0 {
                        0x12
                    } else {
                        0
                    }
                } else {
                    args[1]
                };
                let item = i16::from_be_bytes(args[2..4].try_into().unwrap());
                let handle = u32::from_be_bytes(args[4..8].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("menu mark stack overflow".into()))?;
                Ok((
                    Call::Menu(if opcode == 0xa84f {
                        crate::menus::Op::Command {
                            handle,
                            item,
                            command: mark,
                        }
                    } else {
                        crate::menus::Op::Mark { handle, item, mark }
                    }),
                    Reply::None,
                ))
            }
            0xa94d => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let kind = args[..4].try_into().unwrap();
                let handle = u32::from_be_bytes(args[4..8].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("resource menu stack overflow".into()))?;
                Ok((
                    Call::Menu(crate::menus::Op::AppendResources { handle, kind }),
                    Reply::None,
                ))
            }
            0xa950 => {
                let handle = memory.u32(GuestPtr(sp))?;
                let result = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("menu count stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::Menu(crate::menus::Op::Count(handle)),
                    Reply::StackWord(GuestPtr(result)),
                ))
            }
            0xa93c => {
                let handle = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("menu stack overflow".into()))?;
                Ok((Call::Menu(crate::menus::Op::Set(handle)), Reply::None))
            }
            0xa949 | 0xa9bf | 0xa9c0 => {
                let id = memory.u16(GuestPtr(sp))? as i16;
                let result = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("menu handle stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                c.dar[15] = result;
                Ok((
                    Call::Menu(match opcode {
                        0xa9bf => crate::menus::Op::Load(id),
                        0xa9c0 => crate::menus::Op::Bar(id),
                        _ => crate::menus::Op::Handle(id),
                    }),
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa93b => {
                memory.validate_write(GuestPtr(sp), 4)?;
                Ok((
                    Call::Menu(crate::menus::Op::Snapshot),
                    Reply::StackLong(GuestPtr(sp)),
                ))
            }
            0xa9dc => {
                let args = memory.read(GuestPtr(sp), 6)?;
                let handle = u32::from_be_bytes(args[..4].try_into().unwrap());
                let byte = u16::from_be_bytes(args[4..6].try_into().unwrap()) as u8;
                c.dar[15] = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("TextEdit stack overflow".into()))?;
                Ok((
                    Call::TextEdit(crate::textedit::Op::Key { handle, byte }),
                    Reply::None,
                ))
            }
            0xa9d1 => {
                let args = memory.read(GuestPtr(sp), 12)?;
                let handle = u32::from_be_bytes(args[..4].try_into().unwrap());
                let end = u32::from_be_bytes(args[4..8].try_into().unwrap());
                let start = u32::from_be_bytes(args[8..12].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(12)
                    .ok_or_else(|| Error::Cpu("TextEdit stack overflow".into()))?;
                Ok((
                    Call::TextEdit(crate::textedit::Op::Select { start, end, handle }),
                    Reply::None,
                ))
            }
            0xa9d0 | 0xa9d7 | 0xa9d8 | 0xa9d9 | 0xa9da => {
                let handle = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("TextEdit stack overflow".into()))?;
                Ok((
                    Call::TextEdit(match opcode {
                        0xa9d7 => crate::textedit::Op::Delete(handle),
                        0xa9da => crate::textedit::Op::Idle(handle),
                        0xa9d8 | 0xa9d9 => crate::textedit::Op::Activate {
                            handle,
                            active: opcode == 0xa9d8,
                        },
                        _ => crate::textedit::Op::Calculate(handle),
                    }),
                    Reply::None,
                ))
            }
            0xa9cc => Ok((Call::TextEdit(crate::textedit::Op::Init), Reply::None)),
            0xa9cb | 0xa9cd | 0xa83d => {
                let extra = if opcode == 0xa83d {
                    if memory.u16(GuestPtr(sp))? != 4 {
                        return Err(Error::Cpu("unsupported TextEdit selector".into()));
                    }
                    2
                } else {
                    0
                };
                let handle = memory
                    .u32(GuestPtr(sp.checked_add(extra).ok_or_else(|| {
                        Error::Cpu("TextEdit stack overflow".into())
                    })?))?;
                let result = sp
                    .checked_add(extra + 4)
                    .ok_or_else(|| Error::Cpu("TextEdit stack overflow".into()))?;
                if opcode != 0xa9cd {
                    memory.validate_write(GuestPtr(result), 4)?;
                }
                c.dar[15] = result;
                let op = match opcode {
                    0xa9cb => crate::textedit::Op::GetText(handle),
                    0xa9cd => crate::textedit::Op::Dispose(handle),
                    _ => crate::textedit::Op::GetStyle(handle),
                };
                Ok((
                    Call::TextEdit(op),
                    if opcode == 0xa9cd {
                        Reply::None
                    } else {
                        Reply::StackLong(GuestPtr(result))
                    },
                ))
            }
            0xa861 => {
                memory.validate_write(GuestPtr(sp), 2)?;
                Ok((Call::Random, Reply::StackWord(GuestPtr(sp))))
            }
            0xaa5a => {
                use crate::cfm::Query;
                let selector = memory.u16(GuestPtr(sp))?;
                let size = match selector {
                    3 => 30,
                    4 => 6,
                    5 => 18,
                    6 => 10,
                    7 => 22,
                    _ => return Err(Error::Cpu(format!("unsupported CFM selector {selector}"))),
                };
                let args = memory.read(GuestPtr(sp), size)?;
                let word = |i| u32::from_be_bytes(args[i..i + 4].try_into().unwrap());
                let call = match selector {
                    3 => Call::LoadFragment(crate::cfm::LoadRequest {
                        source: GuestPtr(word(26)),
                        length: word(22),
                        name: GuestPtr(word(18)),
                        options: word(14),
                        connection: GuestPtr(word(10)),
                        main: GuestPtr(word(6)),
                        error: GuestPtr(word(2)),
                    }),
                    4 => Call::CloseFragment(GuestPtr(word(2))),
                    5 => Call::FragmentQuery(Query::Find {
                        connection: word(14),
                        name: GuestPtr(word(10)),
                        address: GuestPtr(word(6)),
                        class: GuestPtr(word(2)),
                    }),
                    6 => Call::FragmentQuery(Query::Count {
                        connection: word(6),
                        count: GuestPtr(word(2)),
                    }),
                    7 => Call::FragmentQuery(Query::Indexed {
                        connection: word(18),
                        index: word(14),
                        name: GuestPtr(word(10)),
                        address: GuestPtr(word(6)),
                        class: GuestPtr(word(2)),
                    }),
                    _ => unreachable!(),
                };
                let result = sp
                    .checked_add(size as u32)
                    .ok_or_else(|| Error::Cpu("CFM stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((call, Reply::StackWord(GuestPtr(result))))
            }
            0xa8e4..=0xa8e7 => {
                use crate::region_geometry::Boolean;
                let args = memory.read(GuestPtr(sp), 12)?;
                let word = |i| u32::from_be_bytes(args[i..i + 4].try_into().unwrap());
                let op = match opcode {
                    0xa8e4 => Boolean::Intersection,
                    0xa8e5 => Boolean::Union,
                    0xa8e6 => Boolean::Difference,
                    _ => Boolean::Xor,
                };
                let call = Call::CombineRegions {
                    op,
                    a: word(8),
                    b: word(4),
                    destination: word(0),
                };
                c.dar[15] = sp
                    .checked_add(12)
                    .ok_or_else(|| Error::Cpu("region stack overflow".into()))?;
                Ok((call, Reply::None))
            }

            0xa8e8 | 0xa8e9 | 0xa8e2 => {
                use crate::regions::Op;
                let pair = opcode != 0xa8e2;
                let n = if pair { 8 } else { 4 };
                let args = memory.read(GuestPtr(sp), n + 2)?;
                let h = u32::from_be_bytes(args[..4].try_into().unwrap());
                let (op, a, b) = if pair {
                    (
                        if opcode == 0xa8e9 {
                            Op::Intersects
                        } else {
                            Op::Contains
                        },
                        u32::from_be_bytes(args[4..8].try_into().unwrap()),
                        h,
                    )
                } else {
                    (Op::Empty, h, 0)
                };
                let result = sp
                    .checked_add(n as u32)
                    .ok_or_else(|| Error::Cpu("region stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 1)?;
                c.dar[15] = result;
                Ok((
                    Call::Region { op, a, b },
                    Reply::StackByte(GuestPtr(result)),
                ))
            }

            0xa8d8 => {
                memory.read(GuestPtr(sp), 4)?;
                Ok((
                    Call::Region {
                        op: crate::regions::Op::New,
                        a: 0,
                        b: 0,
                    },
                    Reply::StackLong(GuestPtr(sp)),
                ))
            }
            0xa8d9 | 0xa8dc | 0xa8df | 0xa879 | 0xa87a => {
                use crate::regions::Op;
                let pair = opcode == 0xa8dc || opcode == 0xa8df;
                let bytes = memory.read(GuestPtr(sp), if pair { 8 } else { 4 })?;
                let first = u32::from_be_bytes(bytes[..4].try_into().unwrap());
                let (a, b) = if pair {
                    (u32::from_be_bytes(bytes[4..8].try_into().unwrap()), first)
                } else {
                    (first, 0)
                };
                c.dar[15] = sp
                    .checked_add(if pair { 8 } else { 4 })
                    .ok_or_else(|| Error::Cpu("region stack overflow".into()))?;
                let op = match opcode {
                    0xa8d9 => Op::Dispose,
                    0xa8dc => Op::Copy,
                    0xa8df => Op::Rect,
                    0xa879 => Op::SetClip,
                    _ => Op::GetClip,
                };
                Ok((Call::Region { op, a, b }, Reply::None))
            }

            0xa870 | 0xa871 | 0xa86d | 0xa86e | 0xa86f | 0xa87d | 0xa873 | 0xa874 | 0xa875
            | 0xa87b | 0xa8a2 | 0xa89d => {
                use crate::quickdraw::PortOp;
                let pointer = GuestPtr(memory.u32(GuestPtr(sp))?);
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("QuickDraw stack overflow".into()))?;
                let call = if opcode == 0xa86e {
                    Call::InitGraf {
                        slot: pointer,
                        anchor: Some(GuestPtr(c.dar[13])),
                    }
                } else {
                    Call::Port {
                        pointer,
                        op: match opcode {
                            0xa870 => PortOp::LocalToGlobal,
                            0xa871 => PortOp::GlobalToLocal,
                            0xa86d => PortOp::Init,
                            0xa86f => PortOp::Open,
                            0xa87d => PortOp::Close,
                            0xa873 => PortOp::Set,
                            0xa874 => PortOp::Get,
                            0xa875 => PortOp::SetBits,
                            0xa87b => PortOp::ClipRect,
                            0xa89d => PortOp::PenPat,
                            _ => PortOp::PaintRect,
                        },
                    }
                };
                Ok((call, Reply::None))
            }
            0xa89b | 0xa876 | 0xa877 => {
                let args = memory.read(GuestPtr(sp), 4)?;
                let height = i16::from_be_bytes(args[..2].try_into().unwrap());
                let width = i16::from_be_bytes(args[2..].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("QuickDraw stack overflow".into()))?;
                Ok((
                    if opcode == 0xa877 {
                        Call::MovePortTo {
                            left: width,
                            top: height,
                        }
                    } else if opcode == 0xa876 {
                        Call::PortSize { width, height }
                    } else {
                        Call::PenSize { width, height }
                    },
                    Reply::None,
                ))
            }
            0xa89c => {
                let mode = memory.u16(GuestPtr(sp))? as i16;
                c.dar[15] = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("QuickDraw stack overflow".into()))?;
                Ok((Call::PenMode(mode), Reply::None))
            }

            0xa860 => {
                let args = memory.read(GuestPtr(sp), 16)?;
                let word = |i| u32::from_be_bytes(args[i..i + 4].try_into().unwrap());
                let call = Call::WaitEvent {
                    region: GuestPtr(word(0)),
                    sleep: word(4),
                    destination: GuestPtr(word(8)),
                    mask: u16::from_be_bytes(args[12..14].try_into().unwrap()),
                };
                let result = sp
                    .checked_add(14)
                    .ok_or_else(|| Error::Cpu("event ABI stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 1)?;
                c.dar[15] = result;
                Ok((call, Reply::StackByte(GuestPtr(result))))
            }

            0xa02f => Ok((
                Call::PostEvent {
                    what: c.dar[8] as u16,
                    message: c.dar[0],
                },
                Reply::M68kFileError,
            )),
            0xa030 | 0xa031 => Ok((
                Call::OSEvent {
                    mask: c.dar[0] as u16,
                    destination: GuestPtr(c.dar[8]),
                    remove: opcode == 0xa031,
                },
                Reply::M68kOSEvent,
            )),
            0xa032 => Ok((
                Call::FlushEvents {
                    which: c.dar[0] as u16,
                    stop: (c.dar[0] >> 16) as u16,
                },
                Reply::M68kD0,
            )),

            0xa647 => Ok((
                Call::SetToolboxTrap {
                    number: c.dar[0] as u16,
                    target: c.dar[8],
                },
                Reply::None,
            )),
            0xa346 | 0xa746 => Ok((
                Call::TrapAddress {
                    number: c.dar[0] as u16,
                    toolbox: opcode & 0x400 != 0,
                },
                Reply::M68kA0,
            )),
            0xa89f => Err(Error::Cpu(
                "unimplemented system routine (dsCoreErr 12)".into(),
            )),
            0xa975 => {
                memory.read(GuestPtr(sp), 4)?;
                Ok((Call::Ticks, Reply::StackLong(GuestPtr(sp))))
            }
            0xa9f4 => Ok((Call::Exit(0), Reply::None)),
            0xa01e | 0xa11e | 0xa31e => Ok((
                Call::NewPointer {
                    size: c.dar[0],
                    clear: opcode & 0x200 != 0,
                },
                Reply::M68kHandle,
            )),
            0xa01f => Ok((Call::DisposePointer(c.dar[8]), Reply::M68kError)),
            0xa020 => Ok((
                Call::SetPointerSize {
                    pointer: c.dar[8],
                    size: c.dar[0],
                },
                Reply::M68kError,
            )),
            0xa021 => Ok((Call::PointerSize(c.dar[8]), Reply::M68kD0)),
            0xa122 | 0xa022 | 0xa322 => Ok((
                Call::NewHandle {
                    size: c.dar[0],
                    clear: opcode & 0x200 != 0,
                },
                Reply::M68kHandle,
            )),
            0xa00d | 0xa20d | 0xa000 | 0xa200 | 0xa00a | 0xa20a | 0xa008 | 0xa208 | 0xa001
            | 0xa002 | 0xa003 | 0xa011 | 0xa012 | 0xa018 | 0xa044 | 0xa045 => {
                use crate::files::{Fork, Op};
                let op = match opcode {
                    0xa00d | 0xa20d => Op::SetInfo {
                        hierarchical: opcode & 0x200 != 0,
                    },
                    0xa000 | 0xa200 | 0xa00a | 0xa20a => Op::Open {
                        kind: if opcode & 0xff == 0 {
                            Fork::Data
                        } else {
                            Fork::Resource
                        },
                        hierarchical: opcode & 0x200 != 0,
                    },
                    0xa008 | 0xa208 => Op::Create {
                        hierarchical: opcode & 0x200 != 0,
                    },
                    0xa001 => Op::Close,
                    0xa002 => Op::Read,
                    0xa003 => Op::Write,
                    0xa011 => Op::GetEof,
                    0xa012 => Op::SetEof,
                    0xa018 => Op::GetPosition,
                    0xa044 => Op::SetPosition,
                    0xa045 => Op::Flush,
                    _ => unreachable!(),
                };
                Ok((
                    Call::FileIo {
                        pointer: GuestPtr(c.dar[8]),
                        op,
                    },
                    Reply::M68kFileError,
                ))
            }
            0xa004 | 0xa204 => Ok((
                Call::DeviceControl(GuestPtr(c.dar[8])),
                Reply::M68kFileError,
            )),
            0xa055 => Ok((Call::StripAddress(c.dar[0]), Reply::M68kD0)),
            0xa029 | 0xa02a => Ok((
                Call::SetHandleLock {
                    handle: c.dar[8],
                    locked: opcode == 0xa029,
                },
                Reply::M68kError,
            )),
            0xa023 => Ok((Call::DisposeHandle(c.dar[8]), Reply::M68kError)),
            0xa024 => Ok((
                Call::SetHandleSize {
                    handle: c.dar[8],
                    size: c.dar[0],
                },
                Reply::M68kError,
            )),
            0xa025 => Ok((Call::HandleSize(c.dar[8]), Reply::M68kD0)),
            0xa9a3 | 0xa992 => {
                let handle = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("resource ABI stack overflow".into()))?;
                Ok((
                    if opcode == 0xa992 {
                        Call::DetachResource(handle)
                    } else {
                        Call::ReleaseResource(handle)
                    },
                    Reply::None,
                ))
            }
            0xa9ce => {
                let args = memory.read(GuestPtr(sp), 14)?;
                let pointer = |i| GuestPtr(u32::from_be_bytes(args[i..i + 4].try_into().unwrap()));
                let request = crate::textedit::TextBox {
                    alignment: i16::from_be_bytes(args[..2].try_into().unwrap()),
                    rectangle: pointer(2),
                    length: i32::from_be_bytes(args[6..10].try_into().unwrap()),
                    text: pointer(10),
                };
                c.dar[15] = sp
                    .checked_add(14)
                    .ok_or_else(|| Error::Cpu("text box stack overflow".into()))?;
                Ok((
                    Call::TextEdit(crate::textedit::Op::TextBox(request)),
                    Reply::None,
                ))
            }
            0xa8a1 | 0xa8a3..=0xa8a5 => {
                use crate::quickdraw::RectangleOp;
                let n = if opcode == 0xa8a5 { 8 } else { 4 };
                let args = memory.read(GuestPtr(sp), n)?;
                let first = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let rectangle = if n == 8 {
                    GuestPtr(u32::from_be_bytes(args[4..8].try_into().unwrap()))
                } else {
                    first
                };
                let op = match opcode {
                    0xa8a1 => RectangleOp::Frame,
                    0xa8a3 => RectangleOp::Erase,
                    0xa8a4 => RectangleOp::Invert,
                    _ => RectangleOp::Fill(first),
                };
                c.dar[15] = sp
                    .checked_add(n as u32)
                    .ok_or_else(|| Error::Cpu("drawing stack overflow".into()))?;
                Ok((Call::DrawRectangle { rectangle, op }, Reply::None))
            }
            0xa8ec => {
                let args = memory.read(GuestPtr(sp), 22)?;
                let pointer = |i| GuestPtr(u32::from_be_bytes(args[i..i + 4].try_into().unwrap()));
                let request = crate::quickdraw::CopyBits {
                    mask: pointer(0),
                    mode: i16::from_be_bytes(args[4..6].try_into().unwrap()),
                    destination_rect: pointer(6),
                    source_rect: pointer(10),
                    destination: pointer(14),
                    source: pointer(18),
                };
                c.dar[15] = sp
                    .checked_add(22)
                    .ok_or_else(|| Error::Cpu("CopyBits stack overflow".into()))?;
                Ok((Call::CopyBits(request), Reply::None))
            }
            0xa8f6 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let destination = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let handle = u32::from_be_bytes(args[4..].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("picture stack overflow".into()))?;
                Ok((
                    Call::DrawPicture {
                        handle,
                        destination,
                    },
                    Reply::None,
                ))
            }
            0xa9b8 | 0xa9b9 | 0xa9bc => {
                let id = memory.u16(GuestPtr(sp))? as i16;
                let result = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("resource ABI stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                c.dar[15] = result;
                Ok((
                    Call::GetResource {
                        kind: match opcode {
                            0xa9b9 => *b"CURS",
                            0xa9bc => *b"PICT",
                            _ => *b"PAT ",
                        },
                        id,
                    },
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa9a0 => {
                let args = memory.read(GuestPtr(sp), 10)?;
                let id = i16::from_be_bytes(args[..2].try_into().unwrap());
                let kind = args[2..6].try_into().unwrap();
                let result = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("resource ABI stack overflow".into()))?;
                c.dar[15] = result;
                Ok((
                    Call::GetResource { kind, id },
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa00c | 0xa20c => Ok((
                Call::FileInfo {
                    pointer: GuestPtr(c.dar[8]),
                    hierarchical: opcode == 0xa20c,
                },
                Reply::M68kFileError,
            )),
            0xa260 if c.dar[0] as u16 == 0x1a => Ok((
                Call::FileIo {
                    pointer: GuestPtr(c.dar[8]),
                    op: crate::files::Op::Open {
                        kind: crate::files::Fork::Data,
                        hierarchical: true,
                    },
                },
                Reply::M68kFileError,
            )),
            0xa260 if c.dar[0] as u16 == 6 => Ok((
                Call::CreateDirectory(GuestPtr(c.dar[8])),
                Reply::M68kFileError,
            )),
            0xa260 if c.dar[0] as u16 == 9 => {
                Ok((Call::CatalogInfo(GuestPtr(c.dar[8])), Reply::M68kFileError))
            }
            0xa260 if c.dar[0] as u16 == 8 => {
                Ok((Call::FcbInfo(GuestPtr(c.dar[8])), Reply::M68kFileError))
            }
            0xa9a1 | 0xa820 => {
                let args = memory.read(GuestPtr(sp), 12)?;
                let name = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let kind = args[4..8].try_into().unwrap();
                let result = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("resource ABI stack overflow".into()))?;
                c.dar[15] = result;
                Ok((
                    Call::GetNamedResource {
                        kind,
                        name,
                        current_only: opcode == 0xa820,
                    },
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa994 => {
                memory.read(GuestPtr(sp), 2)?;
                Ok((Call::CurResFile, Reply::StackWord(GuestPtr(sp))))
            }
            0xa9af => {
                memory.read(GuestPtr(sp), 2)?;
                Ok((Call::ResError, Reply::StackWord(GuestPtr(sp))))
            }
            _ => Err(Error::Service(opcode.into())),
        }
    }
    pub fn import(service: Service, c: &ppc::PpcCpu, memory: &Memory) -> Result<(Call, Reply)> {
        let call = match service {
            Service::RoundRect(frame) => Call::RoundRect {
                pointer: GuestPtr(c.gpr[3]),
                width: c.gpr[4] as i16,
                height: c.gpr[5] as i16,
                frame,
            },
            Service::PortColor(background) => Call::PortColor {
                background,
                color: c.gpr[3],
            },
            Service::NewWindow => Call::NewWindow(crate::windows::NewWindow {
                storage: GuestPtr(c.gpr[3]),
                bounds: GuestPtr(c.gpr[4]),
                title: GuestPtr(c.gpr[5]),
                visible: c.gpr[6] as u8 != 0,
                proc_id: c.gpr[7] as i16,
                behind: c.gpr[8],
                go_away: c.gpr[9] as u8 != 0,
                ref_con: c.gpr[10],
            }),
            Service::Window(op) => Call::Window {
                op,
                pointer: c.gpr[3],
            },
            Service::TextState(field) => Call::TextState {
                field,
                value: c.gpr[3] as u16,
            },
            Service::FontInfo => Call::FontInfo(GuestPtr(c.gpr[3])),
            Service::InitFonts => Call::InitFonts,
            Service::LineTo | Service::Line => Call::PenLine {
                h: c.gpr[3] as i16,
                v: c.gpr[4] as i16,
                relative: service == Service::Line,
            },
            Service::MoveTo => Call::MoveTo {
                h: c.gpr[3] as i16,
                v: c.gpr[4] as i16,
            },
            Service::TextBuffer(width) => {
                let input = crate::text::Input::Buffer {
                    pointer: GuestPtr(c.gpr[3]),
                    first: c.gpr[4] as i16,
                    count: c.gpr[5] as i16,
                };
                if width {
                    Call::TextWidth(input)
                } else {
                    Call::TextDraw(input)
                }
            }
            Service::TextDraw(string) | Service::TextWidth(string) => {
                let input = if string {
                    crate::text::Input::Pascal(GuestPtr(c.gpr[3]))
                } else {
                    crate::text::Input::Character(c.gpr[3] as u8)
                };
                if matches!(service, Service::TextWidth(_)) {
                    Call::TextWidth(input)
                } else {
                    Call::TextDraw(input)
                }
            }
            Service::TENew(styled) => Call::TextEdit(crate::textedit::Op::New {
                dest: GuestPtr(c.gpr[3]),
                view: GuestPtr(c.gpr[4]),
                styled,
            }),
            Service::TEUpdate => Call::TextEdit(crate::textedit::Op::Update {
                rect: GuestPtr(c.gpr[3]),
                handle: c.gpr[4],
            }),
            Service::TESetText => Call::TextEdit(crate::textedit::Op::SetText {
                source: GuestPtr(c.gpr[3]),
                length: c.gpr[4],
                handle: c.gpr[5],
            }),
            Service::TESetSelect => Call::TextEdit(crate::textedit::Op::Select {
                start: c.gpr[3],
                end: c.gpr[4],
                handle: c.gpr[5],
            }),
            Service::TEActivate(active) => Call::TextEdit(crate::textedit::Op::Activate {
                handle: c.gpr[3],
                active,
            }),
            Service::TEIdle => Call::TextEdit(crate::textedit::Op::Idle(c.gpr[3])),
            Service::GetNextEvent | Service::EventAvail => Call::ToolboxEvent {
                mask: c.gpr[3] as u16,
                destination: GuestPtr(c.gpr[4]),
                remove: service == Service::GetNextEvent,
            },
            Service::SetRect => Call::Rectangle(crate::rectangles::Op::Set {
                pointer: GuestPtr(c.gpr[3]),
                edges: [
                    c.gpr[5] as i16,
                    c.gpr[4] as i16,
                    c.gpr[7] as i16,
                    c.gpr[6] as i16,
                ],
            }),
            Service::OffsetRect | Service::InsetRect => {
                Call::Rectangle(if service == Service::OffsetRect {
                    crate::rectangles::Op::Offset {
                        pointer: GuestPtr(c.gpr[3]),
                        dh: c.gpr[4] as i16,
                        dv: c.gpr[5] as i16,
                    }
                } else {
                    crate::rectangles::Op::Inset {
                        pointer: GuestPtr(c.gpr[3]),
                        dh: c.gpr[4] as i16,
                        dv: c.gpr[5] as i16,
                    }
                })
            }
            Service::Gestalt => Call::Gestalt(c.gpr[3]),
            Service::StillDown => Call::StillDown,
            Service::FindWindow => Call::FindWindow {
                point: [(c.gpr[3] >> 16) as i16, c.gpr[3] as i16],
                destination: GuestPtr(c.gpr[4]),
            },
            Service::PtInRect => Call::PointInRect {
                point: [(c.gpr[3] >> 16) as i16, c.gpr[3] as i16],
                rectangle: GuestPtr(c.gpr[4]),
            },
            Service::GetMouse => Call::GetMouse(GuestPtr(c.gpr[3])),
            Service::SystemTask => Call::SystemTask,
            Service::SysEnvirons => Call::SysEnvirons {
                version: c.gpr[3] as i16,
                record: GuestPtr(c.gpr[4]),
            },
            Service::MaxApplZone => Call::MaxApplZone,
            Service::GetApplLimit => Call::GetApplLimit,
            Service::BlockMove => Call::MoveBytes {
                source: GuestPtr(c.gpr[3]),
                destination: GuestPtr(c.gpr[4]),
                count: c.gpr[5] as i32,
            },
            Service::GetDAString => Call::Dialog(crate::dialogs::Op::Slot(c.gpr[3] as i16)),
            Service::InitDialogs => Call::Dialog(crate::dialogs::Op::Init(c.gpr[3])),
            Service::ErrorSound => Call::Dialog(crate::dialogs::Op::Sound(c.gpr[3])),
            Service::SetDialogFont => Call::Dialog(crate::dialogs::Op::Font(c.gpr[3] as i16)),
            Service::ParamText | Service::GetParamText => {
                let pointers = std::array::from_fn(|i| GuestPtr(c.gpr[3 + i]));
                Call::Dialog(if service == Service::ParamText {
                    crate::dialogs::Op::Param(pointers)
                } else {
                    crate::dialogs::Op::Get(pointers)
                })
            }
            Service::Cursor(op) => Call::Cursor(op),
            Service::SetCursor => Call::Cursor(crate::cursor::Op::Set(GuestPtr(c.gpr[3]))),
            Service::GetMenu => Call::Menu(crate::menus::Op::Load(c.gpr[3] as i16)),
            Service::GetNewMBar => Call::Menu(crate::menus::Op::Bar(c.gpr[3] as i16)),
            Service::SetMenuBar => Call::Menu(crate::menus::Op::Set(c.gpr[3])),
            Service::SetMenuItemText => Call::Menu(crate::menus::Op::Text {
                handle: c.gpr[3],
                item: c.gpr[4] as i16,
                source: GuestPtr(c.gpr[5]),
            }),
            Service::SetItemCommand => Call::Menu(crate::menus::Op::Command {
                handle: c.gpr[3],
                item: c.gpr[4] as i16,
                command: c.gpr[5] as u8,
            }),
            Service::SetItemMark | Service::CheckItem => Call::Menu(crate::menus::Op::Mark {
                handle: c.gpr[3],
                item: c.gpr[4] as i16,
                mark: if service == Service::CheckItem {
                    if c.gpr[5] as u8 != 0 {
                        0x12
                    } else {
                        0
                    }
                } else {
                    c.gpr[5] as u8
                },
            }),
            Service::AppendResMenu => Call::Menu(crate::menus::Op::AppendResources {
                handle: c.gpr[3],
                kind: c.gpr[4].to_be_bytes(),
            }),
            Service::CountMenuItems => Call::Menu(crate::menus::Op::Count(c.gpr[3])),
            Service::GetMenuHandle => Call::Menu(crate::menus::Op::Handle(c.gpr[3] as i16)),
            Service::Menu(op) => Call::Menu(op),
            Service::TEKey => Call::TextEdit(crate::textedit::Op::Key {
                handle: c.gpr[4],
                byte: c.gpr[3] as u8,
            }),
            Service::TEDelete => Call::TextEdit(crate::textedit::Op::Delete(c.gpr[3])),
            Service::TECalText => Call::TextEdit(crate::textedit::Op::Calculate(c.gpr[3])),
            Service::TETextBox => {
                Call::TextEdit(crate::textedit::Op::TextBox(crate::textedit::TextBox {
                    text: GuestPtr(c.gpr[3]),
                    length: c.gpr[4] as i32,
                    rectangle: GuestPtr(c.gpr[5]),
                    alignment: c.gpr[6] as i16,
                }))
            }
            Service::TEInit => Call::TextEdit(crate::textedit::Op::Init),
            Service::TEGetText => Call::TextEdit(crate::textedit::Op::GetText(c.gpr[3])),
            Service::TEGetStyleHandle => Call::TextEdit(crate::textedit::Op::GetStyle(c.gpr[3])),
            Service::TEDispose => Call::TextEdit(crate::textedit::Op::Dispose(c.gpr[3])),
            Service::NewControl => Call::NewControl(crate::controls::NewControl {
                owner: GuestPtr(c.gpr[3]),
                bounds: GuestPtr(c.gpr[4]),
                title: GuestPtr(c.gpr[5]),
                visible: c.gpr[6] as u8 != 0,
                value: c.gpr[7] as i16,
                min: c.gpr[8] as i16,
                max: c.gpr[9] as i16,
                proc_id: c.gpr[10] as i16,
                ref_con: memory.u32(GuestPtr(
                    c.gpr[1]
                        .checked_add(56)
                        .ok_or_else(|| Error::Cpu("control PPC stack overflow".into()))?,
                ))?,
            }),
            Service::ControlDraw => Call::Control(crate::controls::Op::Draw(c.gpr[3])),
            Service::ControlDrawAll => Call::Control(crate::controls::Op::DrawAll(c.gpr[3])),
            Service::ControlDispose => Call::Control(crate::controls::Op::Dispose(c.gpr[3])),
            Service::ControlKill => Call::Control(crate::controls::Op::Kill(c.gpr[3])),
            Service::ControlVisible(visible) => {
                Call::Control(crate::controls::Op::Visible(c.gpr[3], visible))
            }
            Service::ControlHilite => {
                Call::Control(crate::controls::Op::Hilite(c.gpr[3], c.gpr[4] as u8))
            }
            Service::ControlValue => Call::Control(crate::controls::Op::Value(c.gpr[3])),
            Service::ControlMin => Call::Control(crate::controls::Op::Min(c.gpr[3])),
            Service::ControlMax => Call::Control(crate::controls::Op::Max(c.gpr[3])),
            Service::ControlSetValue => {
                Call::Control(crate::controls::Op::SetValue(c.gpr[3], c.gpr[4] as i16))
            }
            Service::ControlReference => Call::Control(crate::controls::Op::Reference(c.gpr[3])),
            Service::ControlSetReference => {
                Call::Control(crate::controls::Op::SetReference(c.gpr[3], c.gpr[4]))
            }
            Service::ControlTitle => {
                Call::Control(crate::controls::Op::Title(c.gpr[3], GuestPtr(c.gpr[4])))
            }
            Service::ControlSetTitle => {
                Call::Control(crate::controls::Op::SetTitle(c.gpr[3], GuestPtr(c.gpr[4])))
            }
            Service::ControlFind => Call::Control(crate::controls::Op::Find {
                owner: c.gpr[4],
                point: [(c.gpr[3] >> 16) as i16, c.gpr[3] as i16],
                destination: GuestPtr(c.gpr[5]),
            }),
            Service::ControlTest => Call::Control(crate::controls::Op::Test(
                c.gpr[3],
                c.gpr[4] as i16,
                (c.gpr[4] >> 16) as i16,
            )),
            Service::GetWindowReference => Call::WindowReference {
                pointer: c.gpr[3],
                value: None,
            },
            Service::SetWindowReference => Call::WindowReference {
                pointer: c.gpr[3],
                value: Some(c.gpr[4]),
            },
            Service::InvalRect | Service::ValidRect => Call::WindowInvalidation {
                rectangle: GuestPtr(c.gpr[3]),
                valid: service == Service::ValidRect,
            },
            Service::MoveWindow => Call::WindowChange {
                pointer: c.gpr[3],
                change: crate::windows::Change::Move {
                    horizontal: c.gpr[4] as i16,
                    vertical: c.gpr[5] as i16,
                    front: c.gpr[6] as u8 != 0,
                },
            },
            Service::SelectWindow => Call::WindowChange {
                pointer: c.gpr[3],
                change: crate::windows::Change::Select,
            },
            Service::SizeWindow => Call::WindowChange {
                pointer: c.gpr[3],
                change: crate::windows::Change::Size {
                    width: c.gpr[4] as i16,
                    height: c.gpr[5] as i16,
                    update: c.gpr[6] as u8 != 0,
                },
            },
            Service::ShowHide => Call::WindowChange {
                pointer: c.gpr[3],
                change: crate::windows::Change::Visibility(c.gpr[4] as u8 != 0),
            },
            Service::GetNewWindow => Call::ResourceWindow {
                id: c.gpr[3] as i16,
                storage: GuestPtr(c.gpr[4]),
                behind: c.gpr[5],
            },
            Service::Random => Call::Random,
            Service::GetMemFragment => Call::LoadFragment(crate::cfm::LoadRequest {
                source: GuestPtr(c.gpr[3]),
                length: c.gpr[4],
                name: GuestPtr(c.gpr[5]),
                options: c.gpr[6],
                connection: GuestPtr(c.gpr[7]),
                main: GuestPtr(c.gpr[8]),
                error: GuestPtr(c.gpr[9]),
            }),
            Service::CloseConnection => Call::CloseFragment(GuestPtr(c.gpr[3])),
            Service::CountSymbols => Call::FragmentQuery(crate::cfm::Query::Count {
                connection: c.gpr[3],
                count: GuestPtr(c.gpr[4]),
            }),
            Service::FindSymbol => Call::FragmentQuery(crate::cfm::Query::Find {
                connection: c.gpr[3],
                name: GuestPtr(c.gpr[4]),
                address: GuestPtr(c.gpr[5]),
                class: GuestPtr(c.gpr[6]),
            }),
            Service::GetIndSymbol => Call::FragmentQuery(crate::cfm::Query::Indexed {
                connection: c.gpr[3],
                index: c.gpr[4],
                name: GuestPtr(c.gpr[5]),
                address: GuestPtr(c.gpr[6]),
                class: GuestPtr(c.gpr[7]),
            }),
            Service::Region(op) => Call::Region {
                op,
                a: c.gpr[3],
                b: c.gpr[4],
            },
            Service::CombineRegions(op) => Call::CombineRegions {
                op,
                a: c.gpr[3],
                b: c.gpr[4],
                destination: c.gpr[5],
            },
            Service::InitGraf => Call::InitGraf {
                slot: GuestPtr(c.gpr[3]),
                anchor: None,
            },
            Service::Port(op) => Call::Port {
                op,
                pointer: GuestPtr(c.gpr[3]),
            },
            Service::MovePortTo => Call::MovePortTo {
                left: c.gpr[3] as i16,
                top: c.gpr[4] as i16,
            },
            Service::PortSize => Call::PortSize {
                width: c.gpr[3] as i16,
                height: c.gpr[4] as i16,
            },
            Service::PenSize => Call::PenSize {
                width: c.gpr[3] as i16,
                height: c.gpr[4] as i16,
            },
            Service::PenMode => Call::PenMode(c.gpr[3] as i16),

            Service::WaitNextEvent => Call::WaitEvent {
                mask: c.gpr[3] as u16,
                destination: GuestPtr(c.gpr[4]),
                sleep: c.gpr[5],
                region: GuestPtr(c.gpr[6]),
            },

            Service::PostEvent => Call::PostEvent {
                what: c.gpr[3] as u16,
                message: c.gpr[4],
            },
            Service::GetOSEvent | Service::OSEventAvail => Call::OSEvent {
                mask: c.gpr[3] as u16,
                destination: GuestPtr(c.gpr[4]),
                remove: service == Service::GetOSEvent,
            },
            Service::FlushEvents => Call::FlushEvents {
                which: c.gpr[3] as u16,
                stop: c.gpr[4] as u16,
            },
            Service::SetEventMask => Call::SetEventMask(c.gpr[3] as u16),
            Service::GetEvQHdr => Call::GetEventQueue,

            Service::CExit => Call::CExit(c.gpr[3]),
            Service::BreakPoint => Call::BreakPoint(GuestPtr(c.gpr[3])),
            Service::SetJump => Call::SetJump(GuestPtr(c.gpr[3])),
            Service::LongJump => Call::LongJump {
                buffer: GuestPtr(c.gpr[3]),
                value: c.gpr[4],
            },
            Service::SetToolboxTrapAddress => Call::SetToolboxTrap {
                number: c.gpr[4] as u16,
                target: c.gpr[3],
            },
            Service::GetOSTrapAddress
            | Service::GetToolboxTrapAddress
            | Service::NGetTrapAddress => Call::TrapAddress {
                number: c.gpr[3] as u16,
                toolbox: service == Service::GetToolboxTrapAddress
                    || (service == Service::NGetTrapAddress && c.gpr[4] as u16 != 0),
            },
            Service::TickCount => Call::Ticks,
            Service::ExitToShell => Call::Exit(0),
            Service::NewPointer(clear) => Call::NewPointer {
                size: c.gpr[3],
                clear,
            },
            Service::PointerSize => Call::PointerSize(c.gpr[3]),
            Service::SetPointerSize => Call::SetPointerSize {
                pointer: c.gpr[3],
                size: c.gpr[4],
            },
            Service::DisposePointer => Call::DisposePointer(c.gpr[3]),
            Service::NewHandle | Service::NewHandleClear => Call::NewHandle {
                size: c.gpr[3],
                clear: service == Service::NewHandleClear,
            },
            Service::ReleaseResource => Call::ReleaseResource(c.gpr[3]),
            Service::DetachResource => Call::DetachResource(c.gpr[3]),
            Service::HLock | Service::HUnlock => Call::SetHandleLock {
                handle: c.gpr[3],
                locked: service == Service::HLock,
            },
            Service::DisposeHandle => Call::DisposeHandle(c.gpr[3]),
            Service::SetHandleSize => Call::SetHandleSize {
                handle: c.gpr[3],
                size: c.gpr[4],
            },
            Service::GetHandleSize => Call::HandleSize(c.gpr[3]),
            Service::FrameRect | Service::EraseRect | Service::InvertRect | Service::FillRect => {
                Call::DrawRectangle {
                    rectangle: GuestPtr(c.gpr[3]),
                    op: match service {
                        Service::FrameRect => crate::quickdraw::RectangleOp::Frame,
                        Service::EraseRect => crate::quickdraw::RectangleOp::Erase,
                        Service::InvertRect => crate::quickdraw::RectangleOp::Invert,
                        _ => crate::quickdraw::RectangleOp::Fill(GuestPtr(c.gpr[4])),
                    },
                }
            }
            Service::CopyBits => Call::CopyBits(crate::quickdraw::CopyBits {
                source: GuestPtr(c.gpr[3]),
                destination: GuestPtr(c.gpr[4]),
                source_rect: GuestPtr(c.gpr[5]),
                destination_rect: GuestPtr(c.gpr[6]),
                mode: c.gpr[7] as i16,
                mask: GuestPtr(c.gpr[8]),
            }),
            Service::DrawPicture => Call::DrawPicture {
                handle: c.gpr[3],
                destination: GuestPtr(c.gpr[4]),
            },
            Service::GetPicture => Call::GetResource {
                kind: *b"PICT",
                id: c.gpr[3] as i16,
            },
            Service::GetCursor => Call::GetResource {
                kind: *b"CURS",
                id: c.gpr[3] as i16,
            },
            Service::GetPattern => Call::GetResource {
                kind: *b"PAT ",
                id: c.gpr[3] as i16,
            },
            Service::GetResource => Call::GetResource {
                kind: c.gpr[3].to_be_bytes(),
                id: c.gpr[4] as i16,
            },
            Service::SetSoundVol => Call::SoundVolume(c.gpr[3] as i16),
            Service::GetSoundVol => Call::GetSoundVolume(GuestPtr(c.gpr[3])),
            Service::PBControlSync => Call::DeviceControl(GuestPtr(c.gpr[3])),
            Service::StripAddress => Call::StripAddress(c.gpr[3]),
            Service::MemError => Call::MemError,
            Service::ResError => Call::ResError,
            Service::CurResFile => Call::CurResFile,
            Service::GetNamedResource | Service::Get1NamedResource => Call::GetNamedResource {
                kind: c.gpr[3].to_be_bytes(),
                name: GuestPtr(c.gpr[4]),
                current_only: service == Service::Get1NamedResource,
            },
            Service::FileIoWithAsync(op) => {
                if c.gpr[4] as u8 != 0 {
                    return Err(Error::Cpu(
                        "asynchronous file completion is unsupported".into(),
                    ));
                }
                Call::FileIo {
                    pointer: GuestPtr(c.gpr[3]),
                    op,
                }
            }
            Service::FileIo(op) => Call::FileIo {
                pointer: GuestPtr(c.gpr[3]),
                op,
            },
            Service::FcbInfo => Call::FcbInfo(GuestPtr(c.gpr[3])),
            Service::CatalogInfo => Call::CatalogInfo(GuestPtr(c.gpr[3])),
            Service::FileInfo(hierarchical) => Call::FileInfo {
                pointer: GuestPtr(c.gpr[3]),
                hierarchical,
            },
            Service::CreateDirectory => Call::CreateDirectory(GuestPtr(c.gpr[3])),
        };
        if service == Service::Gestalt {
            return Ok((call, Reply::PpcGestalt(GuestPtr(c.gpr[4]))));
        }
        Ok((
            call,
            if matches!(service, Service::Region(op) if !matches!(op, crate::regions::Op::New | crate::regions::Op::Contains | crate::regions::Op::Intersects | crate::regions::Op::Empty | crate::regions::Op::Equal))
                || matches!(
                    service,
                    Service::GetMouse
                        | Service::SystemTask
                        | Service::InvalRect
                        | Service::ValidRect
                        | Service::MoveWindow
                        | Service::SelectWindow
                        | Service::SetWindowReference
                        | Service::SizeWindow
                        | Service::ShowHide
                        | Service::ControlDraw
                        | Service::ControlDrawAll
                        | Service::ControlDispose
                        | Service::ControlKill
                        | Service::ControlVisible(_)
                        | Service::ControlHilite
                        | Service::ControlSetValue
                        | Service::ControlSetReference
                        | Service::ControlTitle
                        | Service::ControlSetTitle
                        | Service::SetRect
                        | Service::OffsetRect
                        | Service::InsetRect
                        | Service::MaxApplZone
                        | Service::BlockMove
                        | Service::InitDialogs
                        | Service::ParamText
                        | Service::GetParamText
                        | Service::SetDialogFont
                        | Service::ErrorSound
                        | Service::Cursor(_)
                        | Service::SetCursor
                        | Service::SetMenuBar
                        | Service::AppendResMenu
                        | Service::SetItemMark
                        | Service::SetItemCommand
                        | Service::SetMenuItemText
                        | Service::CheckItem
                        | Service::Menu(
                            crate::menus::Op::Init
                                | crate::menus::Op::Draw
                                | crate::menus::Op::Clear
                        )
                        | Service::InitGraf
                        | Service::PortColor(_)
                        | Service::Window(
                            crate::windows::Op::Init
                                | crate::windows::Op::Close
                                | crate::windows::Op::Dispose
                                | crate::windows::Op::Begin
                                | crate::windows::Op::End
                                | crate::windows::Op::Show
                                | crate::windows::Op::Hide
                        )
                        | Service::TEUpdate
                        | Service::TESetText
                        | Service::TECalText
                        | Service::TESetSelect
                        | Service::TEDelete
                        | Service::TEKey
                        | Service::TEIdle
                        | Service::TEActivate(_)
                        | Service::TETextBox
                        | Service::TEInit
                        | Service::TEDispose
                        | Service::LineTo
                        | Service::Line
                        | Service::MoveTo
                        | Service::TextBuffer(false)
                        | Service::TextDraw(_)
                        | Service::InitFonts
                        | Service::FontInfo
                        | Service::TextState(_)
                        | Service::RoundRect(_)
                        | Service::CombineRegions(_)
                        | Service::Port(_)
                        | Service::FrameRect
                        | Service::EraseRect
                        | Service::InvertRect
                        | Service::FillRect
                        | Service::CopyBits
                        | Service::DrawPicture
                        | Service::MovePortTo
                        | Service::PortSize
                        | Service::PenSize
                        | Service::PenMode
                        | Service::ExitToShell
                        | Service::SetPointerSize
                        | Service::DisposePointer
                        | Service::SetHandleSize
                        | Service::DetachResource
                        | Service::ReleaseResource
                        | Service::SetSoundVol
                        | Service::GetSoundVol
                        | Service::HLock
                        | Service::HUnlock
                        | Service::DisposeHandle
                        | Service::FlushEvents
                        | Service::SetToolboxTrapAddress
                        | Service::SetEventMask
                )
            {
                Reply::None
            } else {
                Reply::PpcR3
            },
        ))
    }
}
impl Reply {
    pub fn apply(self, cpu: &mut Cpu, memory: &mut Memory, value: u32, error: i16) -> Result<()> {
        match self {
            Self::None => {}
            Self::StackByte(p) => memory.write(p, &[value as u8])?,
            Self::StackLong(p) => memory.write(p, &value.to_be_bytes())?,
            Self::StackWord(p) => memory.write(p, &(value as u16).to_be_bytes())?,
            Self::M68kGestalt => {
                let Cpu::M68k(c) = cpu else {
                    return Err(Error::Cpu("68K Gestalt result on PPC".into()));
                };
                if error == 0 {
                    c.dar[8] = value;
                }
                Self::M68kFileError.apply(cpu, memory, error as i32 as u32, error)?;
            }
            Self::PpcGestalt(output) => {
                let Cpu::PowerPc(c) = cpu else {
                    return Err(Error::Cpu("PPC Gestalt result on 68K".into()));
                };
                if error == 0 {
                    if output.0 == 0 {
                        return Err(Error::Cpu("nil Gestalt response".into()));
                    }
                    memory.write(output, &value.to_be_bytes())?;
                }
                c.gpr[3] = error as i32 as u32;
            }
            Self::M68kFileError | Self::M68kOSEvent => match cpu {
                Cpu::M68k(c) => {
                    let value = if self == Self::M68kOSEvent {
                        if value == 0 {
                            u32::MAX
                        } else {
                            0
                        }
                    } else {
                        value
                    };
                    c.dar[0] = value;
                    let sr = c.get_sr() & !0xf;
                    c.set_sr(
                        sr | if value as i16 == 0 {
                            4
                        } else if (value as i16) < 0 {
                            8
                        } else {
                            0
                        },
                    );
                }
                _ => return Err(Error::Cpu("68K result on PPC".into())),
            },
            Self::M68kA0 => match cpu {
                Cpu::M68k(c) => c.dar[8] = value,
                _ => return Err(Error::Cpu("68K result on PPC".into())),
            },
            Self::M68kHandle | Self::M68kD0 | Self::M68kError => match cpu {
                Cpu::M68k(c) => {
                    if self == Self::M68kHandle {
                        c.dar[8] = value;
                        c.dar[0] = error as i32 as u32;
                    } else if self == Self::M68kError {
                        c.dar[0] = error as i32 as u32;
                    } else {
                        c.dar[0] = value;
                    }
                }
                _ => return Err(Error::Cpu("68K result on PPC".into())),
            },
            Self::PpcR3 => match cpu {
                Cpu::PowerPc(c) => c.gpr[3] = value,
                _ => return Err(Error::Cpu("PPC result on 68K".into())),
            },
        }
        Ok(())
    }
}
