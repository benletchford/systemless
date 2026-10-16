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
    FixedMultiply,
    LongToFixed,
    FixedRatio,
    SecondsToDate,
    GetDateTime,
    ReadDateTime,
    Timer(u8),
    Retrace { remove: bool, slot: bool },
    NumToString,
    StringToNum,
    Gestalt,
    StillDown,
    FindWindow,
    PtInRect,
    EmptyRect,
    GetMouse,
    Random,
    Logical(crate::integer::Logic),
    TETextBox,
    FrameRect,
    EraseRect,
    InvertRect,
    FillRect,
    LineTo,
    Line,
    CopyBits,
    CopyMask,
    GetNewWindow,
    GetNewColorWindow,
    SizeWindow,
    MoveWindow,
    SelectWindow,
    InvalRect,
    ValidRect,
    GetWindowReference,
    SetWindowReference,
    ShowHide,
    TrackControl,
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
    ControlGeometry(bool),
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
    MoreMasters,
    Device(crate::devices::Op),
    ColorPort(crate::color_ports::Action),
    NextDevice,
    MaxDevice,
    NewDevice,
    DisposeDevice,
    DisposeColorTable,
    GetColorTable,
    RgbToHsl,
    SetEntries,
    GetCTSeed,
    ReserveEntry,
    ProtectEntry,
    SetClientID,
    SetDevice,
    HasDepth,
    SetDepth,
    VblHeader,
    DeviceAttribute,
    SetDeviceAttribute,
    MakeITable,
    QDError,
    Index2Color,
    GetApplLimit,
    SetApplLimit,
    BlockMove,
    InitDialogs,
    DialogItemText(bool),
    DrawDialog,
    SetDialogDefaultItem,
    GetNewDialog,
    GetDialogItem,
    DisposeDialog(bool),
    IsDialogEvent,
    ParamText,
    GetParamText,
    GetDAString,
    SetDialogFont,
    ErrorSound,
    Cursor(crate::cursor::Op),
    SetCursor,
    AppendMenu,
    DisposeMenu,
    NewMenu,
    GetMenuItemText,
    SetMenuItemText,
    SetItemCommand,
    SetItemMark,
    CheckItem,
    EnableItem(bool),
    MenuSelect,
    HiliteMenu,
    MenuChoice,
    Word(bool),
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
    TEGetHeight,
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
    NewColorWindow,
    Window(crate::windows::Op),
    RgbColor(bool),
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
    Polygon(crate::polygons::Op),
    RegionRecording(bool),
    WindowVisibility(bool),
    WindowRepaint(bool),
    PolygonDraw(crate::polygons::Verb),
    KillPoly,
    OffsetPoly,
    FillPoly,
    PenVisibility(i16),
    PenNormal,
    PenMode,
    PenSize,
    PortSize,
    SetOrigin,
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
    HandToHand,
    MoveHHi,
    HGetState,
    HSetState,
    HandleAttribute { resource: bool, enabled: bool },
    HLock,
    HUnlock,
    ReleaseResource,
    DetachResource,
    GetHandleSize,
    SetHandleSize,
    CreateResFile,
    GetResource,
    Get1Resource,
    GetResInfo,
    GetPattern,
    GetCursor,
    GetPicture,
    GetIcon,
    GetWMgrPort,
    SFPutFile,
    SFGetFile,
    DrawPicture,
    SetSoundVol,
    SndNewChannel,
    SndDoImmediate,
    SndDoCommand,
    SndPlay,
    SndStartFilePlay,
    SndSoundManagerVersion,
    SndChannelStatus,
    SndManagerStatus,
    SndDisposeChannel,
    GetSoundVol,
    PBControlSync,
    StripAddress,
    MemError,
    FreeMemory,
    MaxMemory,
    ResError,
    CurResFile,
    OpenSpecResource,
    OpenResourceFile(bool),
    ResourceFile(crate::process::resources::Action),
    GetNamedResource,
    Get1NamedResource,
    FileIo(crate::files::Op),
    FileIoWithAsync(crate::files::Op),
    AddResource,
    CreateSpecResource,
    MakeSpec,
    DeleteSpec,
    GetSpecInfo,
    FlushVolume,
    FlushVolumeWithAsync,
    ResolveAliasFile,
    FcbInfo,
    WorkingDirectory(crate::files::WorkingOp),
    CatalogInfo,
    FileInfo(bool),
    CreateDirectory,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    SaneCompare,
    None,
    StackLong(GuestPtr),
    StackByte(GuestPtr),
    StackWord(GuestPtr),
    PpcR3,
    M68kHandle,
    M68kA0,
    M68kGestalt,
    PpcGestalt(GuestPtr),
    PpcHandleCopy(GuestPtr),
    M68kFileError,
    M68kOSEvent,
    M68kD0,
    M68kMaxMemory(bool),
    M68kByte,
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
            b"SecondsToDate" | b"Secs2Date" => Some(Service::SecondsToDate),
            b"GetDateTime" => Some(Service::GetDateTime),
            b"VInstall" => Some(Service::Retrace {
                remove: false,
                slot: false,
            }),
            b"VRemove" => Some(Service::Retrace {
                remove: true,
                slot: false,
            }),
            b"SlotVInstall" => Some(Service::Retrace {
                remove: false,
                slot: true,
            }),
            b"SlotVRemove" => Some(Service::Retrace {
                remove: true,
                slot: true,
            }),
            b"GetVBLQHdr" => Some(Service::VblHeader),
            b"InsTime" => Some(Service::Timer(0)),
            b"PrimeTime" => Some(Service::Timer(1)),
            b"RmvTime" => Some(Service::Timer(2)),
            b"ReadDateTime" => Some(Service::ReadDateTime),
            b"NumToString" => Some(Service::NumToString),
            b"StringToNum" => Some(Service::StringToNum),
            b"GetMemFragment" => Some(Service::GetMemFragment),
            b"CloseConnection" => Some(Service::CloseConnection),
            b"FrameRoundRect" => Some(Service::RoundRect(true)),
            b"PaintRoundRect" => Some(Service::RoundRect(false)),
            b"RGBForeColor" => Some(Service::RgbColor(false)),
            b"RGBBackColor" => Some(Service::RgbColor(true)),
            b"ForeColor" => Some(Service::PortColor(false)),
            b"BackColor" => Some(Service::PortColor(true)),
            b"TrackControl" => Some(Service::TrackControl),
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
            b"MoveControl" => Some(Service::ControlGeometry(false)),
            b"SizeControl" => Some(Service::ControlGeometry(true)),
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
            b"GetNewCWindow" => Some(Service::GetNewColorWindow),
            b"NewWindow" => Some(Service::NewWindow),
            b"NewCWindow" => Some(Service::NewColorWindow),
            b"ShowWindow" => Some(Service::Window(crate::windows::Op::Show)),
            b"HideWindow" => Some(Service::Window(crate::windows::Op::Hide)),
            b"InitWindows" => Some(Service::Window(crate::windows::Op::Init)),
            b"SFPutFile" => Some(Service::SFPutFile),
            b"SFGetFile" => Some(Service::SFGetFile),
            b"GetWMgrPort" => Some(Service::GetWMgrPort),
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
            b"EmptyRect" => Some(Service::EmptyRect),
            b"PtInRect" => Some(Service::PtInRect),
            b"GetMouse" => Some(Service::GetMouse),
            b"SystemTask" => Some(Service::SystemTask),
            b"SysEnvirons" => Some(Service::SysEnvirons),
            b"OpenCPort" => Some(Service::ColorPort(crate::color_ports::Action::Open)),
            b"InitCPort" => Some(Service::ColorPort(crate::color_ports::Action::Init)),
            b"CloseCPort" => Some(Service::ColorPort(crate::color_ports::Action::Close)),
            b"RGB2HSL" => Some(Service::RgbToHsl),
            b"SetEntries" => Some(Service::SetEntries),
            b"GetCTSeed" => Some(Service::GetCTSeed),
            b"ReserveEntry" => Some(Service::ReserveEntry),
            b"ProtectEntry" => Some(Service::ProtectEntry),
            b"SetClientID" => Some(Service::SetClientID),
            b"SetGDevice" => Some(Service::SetDevice),
            b"GetGDevice" => Some(Service::Device(crate::devices::Op::Current)),
            b"GetMainDevice" => Some(Service::Device(crate::devices::Op::Main)),
            b"GetDeviceList" => Some(Service::Device(crate::devices::Op::First)),
            b"GetCTable" => Some(Service::GetColorTable),
            b"DisposeCTable" => Some(Service::DisposeColorTable),
            b"NewGDevice" => Some(Service::NewDevice),
            b"DisposeGDevice" | b"DisposGDevice" => Some(Service::DisposeDevice),
            b"GetMaxDevice" => Some(Service::MaxDevice),
            b"GetNextDevice" => Some(Service::NextDevice),
            b"HasDepth" => Some(Service::HasDepth),
            b"SetDepth" => Some(Service::SetDepth),
            b"TestDeviceAttribute" => Some(Service::DeviceAttribute),
            b"SetDeviceAttribute" => Some(Service::SetDeviceAttribute),
            b"MakeITable" => Some(Service::MakeITable),
            b"QDError" => Some(Service::QDError),
            b"Index2Color" => Some(Service::Index2Color),
            b"MoreMasters" => Some(Service::MoreMasters),
            b"MaxApplZone" => Some(Service::MaxApplZone),
            b"SetApplLimit" => Some(Service::SetApplLimit),
            b"GetApplLimit" => Some(Service::GetApplLimit),
            b"BlockMove" | b"BlockMoveData" => Some(Service::BlockMove),
            b"IsDialogEvent" => Some(Service::IsDialogEvent),
            b"InitDialogs" => Some(Service::InitDialogs),
            b"SetDialogItemText" | b"SetIText" => Some(Service::DialogItemText(true)),
            b"GetDialogItemText" | b"GetIText" => Some(Service::DialogItemText(false)),
            b"DrawDialog" => Some(Service::DrawDialog),
            b"SetDialogDefaultItem" => Some(Service::SetDialogDefaultItem),
            b"GetNewDialog" => Some(Service::GetNewDialog),
            b"GetDialogItem" | b"GetDItem" => Some(Service::GetDialogItem),
            b"DisposeDialog" | b"DisposDialog" => Some(Service::DisposeDialog(false)),
            b"CloseDialog" => Some(Service::DisposeDialog(true)),
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
            b"AppendMenu" => Some(Service::AppendMenu),
            b"DisposeMenu" => Some(Service::DisposeMenu),
            b"NewMenu" => Some(Service::NewMenu),
            b"GetItem" | b"GetMenuItemText" => Some(Service::GetMenuItemText),
            b"SetItem" | b"SetMenuItemText" => Some(Service::SetMenuItemText),
            b"SetItemCmd" => Some(Service::SetItemCommand),
            b"MenuSelect" => Some(Service::MenuSelect),
            b"HiliteMenu" => Some(Service::HiliteMenu),
            b"HiWord" => Some(Service::Word(true)),
            b"LoWord" => Some(Service::Word(false)),
            b"MenuChoice" => Some(Service::MenuChoice),
            b"EnableItem" => Some(Service::EnableItem(true)),
            b"DisableItem" => Some(Service::EnableItem(false)),
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
            b"TEGetHeight" => Some(Service::TEGetHeight),
            b"TEGetText" => Some(Service::TEGetText),
            b"TEGetStyleHandle" => Some(Service::TEGetStyleHandle),
            b"TEDispose" => Some(Service::TEDispose),
            b"Random" => Some(Service::Random),
            b"FixMul" => Some(Service::FixedMultiply),
            b"Long2Fix" => Some(Service::LongToFixed),
            b"FixRatio" => Some(Service::FixedRatio),
            b"BitAnd" => Some(Service::Logical(crate::integer::Logic::And)),
            b"BitOr" => Some(Service::Logical(crate::integer::Logic::Or)),
            b"BitXor" => Some(Service::Logical(crate::integer::Logic::Xor)),
            b"BitNot" => Some(Service::Logical(crate::integer::Logic::Not)),
            b"FindSymbol" => Some(Service::FindSymbol),
            b"CountSymbols" => Some(Service::CountSymbols),
            b"GetIndSymbol" => Some(Service::GetIndSymbol),
            b"RectInRgn" => Some(Service::Region(crate::regions::Op::Intersects)),
            b"PtInRgn" => Some(Service::Region(crate::regions::Op::Contains)),
            b"EqualRgn" => Some(Service::Region(crate::regions::Op::Equal)),
            b"OffsetRgn" => Some(Service::Region(crate::regions::Op::Offset)),
            b"SetEmptyRgn" => Some(Service::Region(crate::regions::Op::Clear)),
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
            b"SetOrigin" => Some(Service::SetOrigin),
            b"MovePortTo" => Some(Service::MovePortTo),
            b"PortSize" => Some(Service::PortSize),
            b"PenSize" => Some(Service::PenSize),
            b"PaintOne" => Some(Service::WindowRepaint(false)),
            b"PaintBehind" => Some(Service::WindowRepaint(true)),
            b"CalcVis" => Some(Service::WindowVisibility(false)),
            b"CalcVisBehind" => Some(Service::WindowVisibility(true)),
            b"OpenRgn" => Some(Service::RegionRecording(false)),
            b"CloseRgn" => Some(Service::RegionRecording(true)),
            b"OpenPoly" => Some(Service::Polygon(crate::polygons::Op::Open)),
            b"ClosePoly" => Some(Service::Polygon(crate::polygons::Op::Close)),
            b"KillPoly" => Some(Service::KillPoly),
            b"OffsetPoly" => Some(Service::OffsetPoly),
            b"FramePoly" => Some(Service::PolygonDraw(crate::polygons::Verb::Frame)),
            b"PaintPoly" => Some(Service::PolygonDraw(crate::polygons::Verb::Paint)),
            b"ErasePoly" => Some(Service::PolygonDraw(crate::polygons::Verb::Erase)),
            b"InvertPoly" => Some(Service::PolygonDraw(crate::polygons::Verb::Invert)),
            b"FillPoly" => Some(Service::FillPoly),
            b"HidePen" => Some(Service::PenVisibility(-1)),
            b"ShowPen" => Some(Service::PenVisibility(1)),
            b"PenNormal" => Some(Service::PenNormal),
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
            b"HandToHand" => Some(Service::HandToHand),
            b"SetHandleSize" => Some(Service::SetHandleSize),
            b"GetHandleSize" => Some(Service::GetHandleSize),
            b"ReleaseResource" => Some(Service::ReleaseResource),
            b"DetachResource" => Some(Service::DetachResource),
            b"SndDoImmediate" => Some(Service::SndDoImmediate),
            b"SndDoCommand" => Some(Service::SndDoCommand),
            b"SndPlay" => Some(Service::SndPlay),
            b"SndStartFilePlay" => Some(Service::SndStartFilePlay),
            b"SndSoundManagerVersion" => Some(Service::SndSoundManagerVersion),
            b"SndChannelStatus" => Some(Service::SndChannelStatus),
            b"SndManagerStatus" => Some(Service::SndManagerStatus),
            b"SndNewChannel" => Some(Service::SndNewChannel),
            b"SndDisposeChannel" => Some(Service::SndDisposeChannel),
            b"SetSoundVol" => Some(Service::SetSoundVol),
            b"GetSoundVol" => Some(Service::GetSoundVol),
            b"PBControlSync" => Some(Service::PBControlSync),
            b"StripAddress" => Some(Service::StripAddress),
            b"MoveHHi" => Some(Service::MoveHHi),
            b"HGetState" => Some(Service::HGetState),
            b"HSetState" => Some(Service::HSetState),
            b"HPurge" => Some(Service::HandleAttribute {
                resource: false,
                enabled: true,
            }),
            b"HNoPurge" => Some(Service::HandleAttribute {
                resource: false,
                enabled: false,
            }),
            b"HSetRBit" => Some(Service::HandleAttribute {
                resource: true,
                enabled: true,
            }),
            b"HClrRBit" => Some(Service::HandleAttribute {
                resource: true,
                enabled: false,
            }),
            b"HLock" => Some(Service::HLock),
            b"HUnlock" => Some(Service::HUnlock),
            b"CopyMask" => Some(Service::CopyMask),
            b"CopyBits" => Some(Service::CopyBits),
            b"DrawPicture" => Some(Service::DrawPicture),
            b"GetIcon" => Some(Service::GetIcon),
            b"GetPicture" => Some(Service::GetPicture),
            b"GetCursor" => Some(Service::GetCursor),
            b"GetPattern" => Some(Service::GetPattern),
            b"CreateResFile" => Some(Service::CreateResFile),
            b"GetResInfo" => Some(Service::GetResInfo),
            b"GetResource" => Some(Service::GetResource),
            b"Get1Resource" => Some(Service::Get1Resource),
            b"FreeMem" => Some(Service::FreeMemory),
            b"MaxMem" => Some(Service::MaxMemory),
            b"MemError" => Some(Service::MemError),
            b"PBOpenSync" | b"PBOpenDFSync" => Some(Service::FileIo(crate::files::Op::Open {
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
            b"PBDeleteSync" => Some(Service::FileIo(crate::files::Op::Delete {
                hierarchical: false,
            })),
            b"PBHDeleteSync" => Some(Service::FileIo(crate::files::Op::Delete {
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
            b"PBSetVolSync" => Some(Service::WorkingDirectory(crate::files::WorkingOp::Set {
                hierarchical: false,
            })),
            b"PBHSetVolSync" => Some(Service::WorkingDirectory(crate::files::WorkingOp::Set {
                hierarchical: true,
            })),
            b"PBGetVolSync" => Some(Service::WorkingDirectory(
                crate::files::WorkingOp::Current {
                    hierarchical: false,
                },
            )),
            b"PBHGetVolSync" => Some(Service::WorkingDirectory(
                crate::files::WorkingOp::Current { hierarchical: true },
            )),
            b"PBOpenWDSync" => Some(Service::WorkingDirectory(crate::files::WorkingOp::Open)),
            b"PBCloseWDSync" => Some(Service::WorkingDirectory(crate::files::WorkingOp::Close)),
            b"PBGetWDInfoSync" => Some(Service::WorkingDirectory(crate::files::WorkingOp::Info)),
            b"UpdateResFile" => Some(Service::ResourceFile(
                crate::process::resources::Action::Update,
            )),
            b"AddResource" => Some(Service::AddResource),
            b"FSpCreateResFile" => Some(Service::CreateSpecResource),
            b"FSMakeFSSpec" => Some(Service::MakeSpec),
            b"FSpDelete" => Some(Service::DeleteSpec),
            b"FSpGetFInfo" => Some(Service::GetSpecInfo),
            b"PBFlushVolSync" => Some(Service::FlushVolume),
            b"PBFlushVol" => Some(Service::FlushVolumeWithAsync),
            b"ResolveAliasFile" => Some(Service::ResolveAliasFile),
            b"PBGetFCBInfoSync" => Some(Service::FcbInfo),
            b"PBGetCatInfoSync" => Some(Service::CatalogInfo),
            b"PBGetFInfoSync" => Some(Service::FileInfo(false)),
            b"PBHGetFInfoSync" => Some(Service::FileInfo(true)),
            b"PBDirCreateSync" => Some(Service::CreateDirectory),
            b"GetNamedResource" => Some(Service::GetNamedResource),
            b"Get1NamedResource" => Some(Service::Get1NamedResource),
            b"OpenRFPerm" => Some(Service::OpenResourceFile(true)),
            b"FSpOpenResFile" => Some(Service::OpenSpecResource),
            b"OpenResFile" => Some(Service::OpenResourceFile(false)),
            b"UseResFile" => Some(Service::ResourceFile(
                crate::process::resources::Action::Use,
            )),
            b"CloseResFile" => Some(Service::ResourceFile(
                crate::process::resources::Action::Close,
            )),
            b"HomeResFile" => Some(Service::ResourceFile(
                crate::process::resources::Action::Home,
            )),
            b"CurResFile" => Some(Service::CurResFile),
            b"ResError" => Some(Service::ResError),
            _ => None,
        }
    }
    pub fn trap(opcode: u16, c: &mut m68k::CpuCore, memory: &Memory) -> Result<(Call, Reply)> {
        let sp = c.dar[15];
        match opcode {
            0xa033 | 0xa034 | 0xa06f | 0xa070 => Ok((
                Call::Retrace(crate::retrace::Op {
                    task: GuestPtr(c.dar[8]),
                    slot: if opcode >= 0xa06f {
                        Some(c.dar[0] as i16)
                    } else {
                        None
                    },
                    remove: matches!(opcode, 0xa034 | 0xa070),
                }),
                Reply::M68kError,
            )),
            0xa058..=0xa05a => Ok((
                Call::Timer(crate::timers::Op {
                    task: GuestPtr(c.dar[8]),
                    action: match opcode {
                        0xa058 => crate::timers::Action::Insert,
                        0xa05a => crate::timers::Action::Prime(c.dar[0] as i32),
                        _ => crate::timers::Action::Remove,
                    },
                }),
                Reply::M68kError,
            )),
            0xa9c6 => Ok((
                Call::SecondsToDate {
                    seconds: c.dar[0],
                    destination: GuestPtr(c.dar[8]),
                },
                Reply::None,
            )),
            0xa039 => Ok((Call::ReadDateTime(GuestPtr(c.dar[8])), Reply::M68kError)),
            0xa9ee => {
                let selector = memory.u16(GuestPtr(sp))?;
                let (op, reply) = match selector {
                    0 => (
                        crate::decimal::Op::Format {
                            number: c.dar[0] as i32,
                            destination: GuestPtr(c.dar[8]),
                        },
                        Reply::None,
                    ),
                    1 => (
                        crate::decimal::Op::Parse {
                            source: GuestPtr(c.dar[8]),
                            destination: None,
                        },
                        Reply::M68kD0,
                    ),
                    _ => return Err(Error::Service(u32::from(opcode))),
                };
                c.dar[15] = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("decimal selector stack overflow".into()))?;
                Ok((Call::Decimal(op), reply))
            }
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
            0xa968 => {
                let args = memory.read(GuestPtr(sp), 12)?;
                let action = u32::from_be_bytes(args[..4].try_into().unwrap());
                let point = [
                    i16::from_be_bytes(args[4..6].try_into().unwrap()),
                    i16::from_be_bytes(args[6..8].try_into().unwrap()),
                ];
                let handle = u32::from_be_bytes(args[8..12].try_into().unwrap());
                let result = sp
                    .checked_add(12)
                    .ok_or_else(|| Error::Cpu("control tracking stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::TrackControl(crate::controls::TrackControl {
                        handle,
                        point,
                        action,
                    }),
                    Reply::StackWord(GuestPtr(result)),
                ))
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
            0xa959 | 0xa95c => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let v = i16::from_be_bytes(args[..2].try_into().unwrap());
                let h = i16::from_be_bytes(args[2..4].try_into().unwrap());
                let handle = u32::from_be_bytes(args[4..].try_into().unwrap());
                let change = if opcode == 0xa959 {
                    crate::controls::Geometry::Move { h, v }
                } else {
                    crate::controls::Geometry::Size {
                        width: h,
                        height: v,
                    }
                };
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("control geometry stack overflow".into()))?;
                Ok((
                    Call::Control(crate::controls::Op::Geometry { handle, change }),
                    Reply::None,
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
            0xa9bd | 0xaa46 => {
                let args = memory.read(GuestPtr(sp), 14)?;
                let result = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                let call = Call::ResourceWindow(crate::windows::ResourceWindow {
                    id: i16::from_be_bytes(args[8..10].try_into().unwrap()),
                    storage: GuestPtr(u32::from_be_bytes(args[4..8].try_into().unwrap())),
                    behind: u32::from_be_bytes(args[..4].try_into().unwrap()),
                    color: opcode == 0xaa46,
                });
                c.dar[15] = result;
                Ok((call, Reply::StackLong(GuestPtr(result))))
            }
            0xa868 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let right = i32::from_be_bytes(args[..4].try_into().unwrap());
                let left = i32::from_be_bytes(args[4..].try_into().unwrap());
                let next = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("FixMul stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 4)?;
                c.dar[15] = next;
                Ok((
                    Call::FixedMultiply { left, right },
                    Reply::StackLong(GuestPtr(next)),
                ))
            }
            0xa9ab => {
                let args = memory.read(GuestPtr(sp), 14)?;
                let request = crate::process::resources::AddResource {
                    name: GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap())),
                    id: i16::from_be_bytes(args[4..6].try_into().unwrap()),
                    kind: args[6..10].try_into().unwrap(),
                    handle: u32::from_be_bytes(args[10..14].try_into().unwrap()),
                };
                request.validate(memory)?;
                let next = sp
                    .checked_add(14)
                    .ok_or_else(|| Error::Cpu("AddResource stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::AddResource(request), Reply::None))
            }
            0xaa52 if c.dar[0] as u16 == 13 => {
                let args = memory.read(GuestPtr(sp), 6)?;
                let spec = GuestPtr(u32::from_be_bytes(args[2..6].try_into().unwrap()));
                if spec.0 == 0 {
                    return Err(Error::Cpu("nil resource FSSpec".into()));
                }
                memory.read(spec, 70)?;
                memory.validate_write(GuestPtr(0xa60), 2)?;
                memory.validate_write(GuestPtr(0xa5a), 2)?;
                let next = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("FSpOpenResFile stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 2)?;
                c.dar[15] = next;
                Ok((
                    Call::ResourceFile(crate::process::resources::Op::OpenSpec {
                        spec,
                        permission: args[0] as i8,
                    }),
                    Reply::StackWord(GuestPtr(next)),
                ))
            }
            0xaa52 if c.dar[0] as u16 == 14 => {
                let args = memory.read(GuestPtr(sp), 14)?;
                let request = crate::files::CreateSpecResource {
                    script: i16::from_be_bytes(args[..2].try_into().unwrap()),
                    kind: args[2..6].try_into().unwrap(),
                    creator: args[6..10].try_into().unwrap(),
                    spec: GuestPtr(u32::from_be_bytes(args[10..14].try_into().unwrap())),
                };
                request.validate(memory)?;
                memory.validate_write(GuestPtr(0xa60), 2)?;
                let next = sp
                    .checked_add(14)
                    .ok_or_else(|| Error::Cpu("FSpCreateResFile stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::CreateSpecResource(request), Reply::None))
            }
            0xaa52 if c.dar[0] as u16 == 7 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let request = crate::files::GetSpecInfo {
                    destination: GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap())),
                    spec: GuestPtr(u32::from_be_bytes(args[4..8].try_into().unwrap())),
                };
                request.validate(memory)?;
                let next = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("FSpGetFInfo stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 2)?;
                c.dar[15] = next;
                Ok((Call::GetSpecInfo(request), Reply::StackWord(GuestPtr(next))))
            }
            0xaa52 if c.dar[0] as u16 == 6 => {
                let spec = GuestPtr(memory.u32(GuestPtr(sp))?);
                if spec.0 == 0 {
                    return Err(Error::Cpu("nil deletion FSSpec".into()));
                }
                memory.read(spec, 70)?;
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("FSpDelete stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 2)?;
                c.dar[15] = next;
                Ok((Call::DeleteSpec(spec), Reply::StackWord(GuestPtr(next))))
            }
            0xaa52 => {
                if c.dar[0] as u16 != 1 {
                    return Err(Error::Cpu(format!(
                        "unsupported HighLevelFSDispatch selector {:04x}",
                        c.dar[0] as u16
                    )));
                }
                let args = memory.read(GuestPtr(sp), 14)?;
                let request = crate::files::MakeSpec {
                    destination: GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap())),
                    name: GuestPtr(u32::from_be_bytes(args[4..8].try_into().unwrap())),
                    directory: u32::from_be_bytes(args[8..12].try_into().unwrap()),
                    volume: i16::from_be_bytes(args[12..14].try_into().unwrap()),
                };
                let next = sp
                    .checked_add(14)
                    .ok_or_else(|| Error::Cpu("FSMakeFSSpec stack overflow".into()))?;
                request.validate(memory)?;
                memory.validate_write(GuestPtr(next), 2)?;
                c.dar[15] = next;
                Ok((Call::MakeSpec(request), Reply::StackWord(GuestPtr(next))))
            }
            0xa823 => {
                if c.dar[0] as u16 != 12 {
                    return Err(Error::Cpu(format!(
                        "unsupported AliasDispatch selector {:04x}",
                        c.dar[0] as u16
                    )));
                }
                let args = memory.read(GuestPtr(sp), 14)?;
                let request = crate::files::ResolveAliasFile {
                    spec: GuestPtr(u32::from_be_bytes(args[10..14].try_into().unwrap())),
                    chains: args[8] != 0,
                    folder: GuestPtr(u32::from_be_bytes(args[4..8].try_into().unwrap())),
                    aliased: GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap())),
                };
                let next = sp
                    .checked_add(14)
                    .ok_or_else(|| Error::Cpu("ResolveAliasFile stack overflow".into()))?;
                request.validate(memory)?;
                memory.validate_write(GuestPtr(next), 2)?;
                c.dar[15] = next;
                Ok((
                    Call::ResolveAliasFile(request),
                    Reply::StackWord(GuestPtr(next)),
                ))
            }
            0xa83f => {
                let input = memory.u32(GuestPtr(sp))? as i32;
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("Long2Fix stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 4)?;
                c.dar[15] = next;
                Ok((Call::LongToFixed(input), Reply::StackLong(GuestPtr(next))))
            }
            0xa869 => {
                let args = memory.read(GuestPtr(sp), 4)?;
                let denominator = i16::from_be_bytes(args[..2].try_into().unwrap());
                let numerator = i16::from_be_bytes(args[2..4].try_into().unwrap());
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("FixRatio stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 4)?;
                c.dar[15] = next;
                Ok((
                    Call::FixedRatio {
                        numerator,
                        denominator,
                    },
                    Reply::StackLong(GuestPtr(next)),
                ))
            }
            0xa858..=0xa85b => {
                use crate::integer::Logic;
                let op = match opcode {
                    0xa858 => Logic::And,
                    0xa859 => Logic::Xor,
                    0xa85a => Logic::Not,
                    _ => Logic::Or,
                };
                let size = if op == Logic::Not { 4 } else { 8 };
                let bytes = memory.read(GuestPtr(sp), size)?;
                let first = u32::from_be_bytes(bytes[..4].try_into().unwrap());
                let (left, right) = if size == 4 {
                    (first, 0)
                } else {
                    (u32::from_be_bytes(bytes[4..8].try_into().unwrap()), first)
                };
                let result = sp
                    .checked_add(size as u32)
                    .ok_or_else(|| Error::Cpu("logical utility stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                c.dar[15] = result;
                Ok((
                    Call::Logical { op, left, right },
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xaa31 => {
                let device = memory.u32(GuestPtr(sp))?;
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("SetGDevice stack overflow".into()))?;
                c.dar[15] = next;
                Ok((
                    Call::Device(crate::devices::Op::Select(device)),
                    Reply::None,
                ))
            }
            0xa800 => {
                let selector = c.dar[0];
                match selector & 0x00ff_ffff {
                    0x00000008 if selector >> 24 == 13 => {
                        let b = memory.read(GuestPtr(sp), 26)?;
                        let long =
                            |at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap());
                        let word =
                            |at: usize| i16::from_be_bytes(b[at..at + 2].try_into().unwrap());
                        let request = crate::sound::StartFilePlay {
                            channel: long(22),
                            file: word(20),
                            resource: word(18),
                            buffer_size: long(14) as i32,
                            buffer: GuestPtr(long(10)),
                            selection: GuestPtr(long(6)),
                            completion: long(2),
                            asynchronous: b[0] != 0,
                        };
                        let result = sp
                            .checked_add(26)
                            .ok_or_else(|| Error::Cpu("SndStartFilePlay stack overflow".into()))?;
                        memory.validate_write(GuestPtr(result), 2)?;
                        c.dar[15] = result;
                        Ok((
                            Call::SoundStartFile(request),
                            Reply::StackWord(GuestPtr(result)),
                        ))
                    }
                    0x000c0008 if selector >> 24 == 0 => {
                        memory.validate_write(GuestPtr(sp), 4)?;
                        Ok((Call::SoundVersion, Reply::StackLong(GuestPtr(sp))))
                    }
                    0x00100008 | 0x00140008
                        if selector >> 24 == 0
                            || selector >> 24
                                == if selector & 0x00ff_ffff == 0x00100008 {
                                    5
                                } else {
                                    3
                                } =>
                    {
                        let size = if selector & 0x00ff_ffff == 0x00100008 {
                            10
                        } else {
                            6
                        };
                        let bytes = memory.read(GuestPtr(sp), size)?;
                        let result = sp
                            .checked_add(size as u32)
                            .ok_or_else(|| Error::Cpu("sound status stack overflow".into()))?;
                        memory.validate_write(GuestPtr(result), 2)?;
                        let request = crate::sound::Status {
                            channel: if size == 10 {
                                Some(u32::from_be_bytes(bytes[6..10].try_into().unwrap()))
                            } else {
                                None
                            },
                            length: i16::from_be_bytes(bytes[4..6].try_into().unwrap()),
                            destination: GuestPtr(u32::from_be_bytes(
                                bytes[..4].try_into().unwrap(),
                            )),
                        };
                        c.dar[15] = result;
                        Ok((
                            Call::SoundStatus(request),
                            Reply::StackWord(GuestPtr(result)),
                        ))
                    }
                    _ => Err(Error::Cpu(format!(
                        "unsupported SoundDispatch selector {selector:#010x}"
                    ))),
                }
            }
            0xa805 => {
                let args = memory.read(GuestPtr(sp), 10)?;
                let handle = u32::from_be_bytes(args[2..6].try_into().unwrap());
                let channel = u32::from_be_bytes(args[6..10].try_into().unwrap());
                let result = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("resource sound stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::SoundPlay {
                        channel,
                        handle,
                        asynchronous: args[0] != 0,
                    },
                    Reply::StackWord(GuestPtr(result)),
                ))
            }
            0xa803 => {
                let args = memory.read(GuestPtr(sp), 10)?;
                let command = GuestPtr(u32::from_be_bytes(args[2..6].try_into().unwrap()));
                let channel = u32::from_be_bytes(args[6..10].try_into().unwrap());
                let result = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("queued sound stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::SoundQueue {
                        channel,
                        command,
                        no_wait: args[0] != 0,
                    },
                    Reply::StackWord(GuestPtr(result)),
                ))
            }
            0xa804 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let command = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let channel = u32::from_be_bytes(args[4..8].try_into().unwrap());
                let result = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("sound command stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::SoundImmediate { channel, command },
                    Reply::StackWord(GuestPtr(result)),
                ))
            }
            0xa913 | 0xaa45 => {
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
                let call = if opcode == 0xaa45 {
                    Call::NewColorWindow(request)
                } else {
                    Call::NewWindow(request)
                };
                Ok((call, Reply::StackLong(GuestPtr(result))))
            }
            0xa9ea => {
                let selector = memory.u16(GuestPtr(sp))?;
                if selector == 2 {
                    let bytes = memory.read(GuestPtr(sp), 28)?;
                    let long = |n| u32::from_be_bytes(bytes[n..n + 4].try_into().unwrap());
                    let request = crate::standard_file::GetFile {
                        reply: GuestPtr(long(2)),
                        hook: long(6),
                        types: GuestPtr(long(10)),
                        count: i16::from_be_bytes(bytes[14..16].try_into().unwrap()),
                        filter: long(16),
                        point: [
                            i16::from_be_bytes(bytes[24..26].try_into().unwrap()),
                            i16::from_be_bytes(bytes[26..28].try_into().unwrap()),
                        ],
                    };
                    request.validate(memory)?;
                    let next = sp
                        .checked_add(28)
                        .ok_or_else(|| Error::Cpu("SFGetFile stack overflow".into()))?;
                    c.dar[15] = next;
                    return Ok((Call::StandardGetFile(request), Reply::None));
                }
                if selector != 1 {
                    return Err(Error::Cpu(format!(
                        "unsupported Standard File selector {selector:#06x}"
                    )));
                }
                let bytes = memory.read(GuestPtr(sp), 22)?;
                let long = |n| u32::from_be_bytes(bytes[n..n + 4].try_into().unwrap());
                let request = crate::standard_file::PutFile {
                    reply: GuestPtr(long(2)),
                    hook: long(6),
                    name: GuestPtr(long(10)),
                    prompt: GuestPtr(long(14)),
                    point: [
                        i16::from_be_bytes(bytes[18..20].try_into().unwrap()),
                        i16::from_be_bytes(bytes[20..22].try_into().unwrap()),
                    ],
                };
                memory.validate_write(request.reply, 74)?;
                if request.hook != 0 {
                    return Err(Error::Cpu(
                        "SFPutFile dialog hooks require guest callbacks".into(),
                    ));
                }
                let next = sp
                    .checked_add(22)
                    .ok_or_else(|| Error::Cpu("SFPutFile stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::StandardPutFile(request), Reply::None))
            }
            0xa910 => {
                let destination = GuestPtr(memory.u32(GuestPtr(sp))?);
                memory.validate_write(destination, 4)?;
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("GetWMgrPort stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::GetWMgrPort(destination), Reply::None))
            }
            0xa90c | 0xa90d => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let request = crate::windows::Repaint {
                    window: u32::from_be_bytes(args[4..8].try_into().unwrap()),
                    clobbered: u32::from_be_bytes(args[..4].try_into().unwrap()),
                    behind: opcode == 0xa90d,
                };
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("repaint stack overflow".into()))?;
                Ok((Call::WindowRepaint(request), Reply::None))
            }
            0xa909 | 0xa90a => {
                let behind = opcode == 0xa90a;
                let n = if behind { 8 } else { 4 };
                let args = memory.read(GuestPtr(sp), n)?;
                let request = if behind {
                    crate::windows::Visibility::Behind {
                        start: u32::from_be_bytes(args[4..8].try_into().unwrap()),
                        clobbered: u32::from_be_bytes(args[..4].try_into().unwrap()),
                    }
                } else {
                    crate::windows::Visibility::Window(u32::from_be_bytes(args.try_into().unwrap()))
                };
                c.dar[15] = sp
                    .checked_add(n as u32)
                    .ok_or_else(|| Error::Cpu("visibility stack overflow".into()))?;
                Ok((Call::WindowVisibility(request), Reply::None))
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
            0xa8ae => {
                let rectangle = GuestPtr(memory.u32(GuestPtr(sp))?);
                let result = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("EmptyRect stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 1)?;
                c.dar[15] = result;
                Ok((
                    Call::EmptyRect(rectangle),
                    Reply::StackByte(GuestPtr(result)),
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
            0xaa00..=0xaa02 => {
                let port = GuestPtr(memory.u32(GuestPtr(sp))?);
                if port.0 == 0 {
                    return Err(Error::Graphics("nil color port".into()));
                }
                let action = match opcode {
                    0xaa00 => crate::color_ports::Action::Open,
                    0xaa01 => crate::color_ports::Action::Init,
                    _ => crate::color_ports::Action::Close,
                };
                match action {
                    crate::color_ports::Action::Open => memory.validate_write(port, 108)?,
                    crate::color_ports::Action::Init => {
                        let bytes = memory.read(port, 108)?;
                        if u16::from_be_bytes(bytes[6..8].try_into().unwrap()) & 0xc000 == 0xc000 {
                            memory.validate_write(port, 108)?;
                        }
                    }
                    crate::color_ports::Action::Close => {
                        memory.read(port, 108)?;
                    }
                }
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("color port ABI stack overflow".into()))?;
                Ok((Call::ColorPort { action, port }, Reply::None))
            }
            0xaaa2 => {
                let selector = c.dar[0] as u16;
                if !matches!(selector, 0x0a13 | 0x0a14) {
                    return Err(Error::Service(u32::from(opcode)));
                }
                let args = memory.read(GuestPtr(sp), 12)?;
                let next = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("display depth stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 2)?;
                let op = crate::devices::Op::Depth {
                    flags: u16::from_be_bytes(args[0..2].try_into().unwrap()),
                    which_flags: u16::from_be_bytes(args[2..4].try_into().unwrap()),
                    depth: i16::from_be_bytes(args[4..6].try_into().unwrap()),
                    device: u32::from_be_bytes(args[6..10].try_into().unwrap()),
                    set: selector == 0x0a13,
                };
                c.dar[15] = next;
                Ok((Call::Device(op), Reply::StackWord(GuestPtr(next))))
            }
            0xa82e => {
                if memory.u16(GuestPtr(sp))? != 6 {
                    return Err(Error::Service(u32::from(opcode)));
                }
                let args = memory.read(GuestPtr(sp), 10)?;
                let destination = GuestPtr(u32::from_be_bytes(args[2..6].try_into().unwrap()));
                let source = GuestPtr(u32::from_be_bytes(args[6..10].try_into().unwrap()));
                if source.0 == 0 || destination.0 == 0 {
                    return Err(Error::Graphics("nil Color Picker conversion record".into()));
                }
                memory.read(source, 6)?;
                memory.validate_write(destination, 6)?;
                let next = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("Color Picker stack overflow".into()))?;
                c.dar[15] = next;
                Ok((
                    Call::Color(crate::color::Op::RgbToHsl {
                        source,
                        destination,
                    }),
                    Reply::None,
                ))
            }
            0xaa3f => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let op = crate::color::Op::SetEntries {
                    source: GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap())),
                    count: i16::from_be_bytes(args[4..6].try_into().unwrap()),
                    start: i16::from_be_bytes(args[6..8].try_into().unwrap()),
                };
                let next = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("SetEntries stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::Color(op), Reply::None))
            }
            0xaa28 => {
                memory.validate_write(GuestPtr(sp), 4)?;
                Ok((
                    Call::Color(crate::color::Op::Seed),
                    Reply::StackLong(GuestPtr(sp)),
                ))
            }
            0xaa3d | 0xaa3e => {
                let args = memory.read(GuestPtr(sp), 4)?;
                let index = i16::from_be_bytes(args[2..4].try_into().unwrap());
                let op = if opcode == 0xaa3d {
                    crate::color::Op::Protect {
                        index,
                        protect: args[0] != 0,
                    }
                } else {
                    crate::color::Op::Reserve {
                        index,
                        reserve: args[0] != 0,
                    }
                };
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("ProtectEntry stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::Color(op), Reply::None))
            }
            0xaa40 => {
                memory.validate_write(GuestPtr(sp), 2)?;
                Ok((
                    Call::Color(crate::color::Op::Error),
                    Reply::StackWord(GuestPtr(sp)),
                ))
            }
            0xaa39 => {
                let args = memory.read(GuestPtr(sp), 10)?;
                let op = crate::color::Op::Make {
                    source: u32::from_be_bytes(args[6..10].try_into().unwrap()),
                    destination: u32::from_be_bytes(args[2..6].try_into().unwrap()),
                    resolution: i16::from_be_bytes(args[..2].try_into().unwrap()),
                };
                let next = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("MakeITable stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::Color(op), Reply::None))
            }
            0xaa2d => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let next = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("device attribute stack overflow".into()))?;
                let op = crate::devices::Op::SetAttribute {
                    device: u32::from_be_bytes(args[4..8].try_into().unwrap()),
                    bit: i16::from_be_bytes(args[2..4].try_into().unwrap()),
                    value: args[0] != 0,
                };
                c.dar[15] = next;
                Ok((Call::Device(op), Reply::None))
            }
            0xaa2f => {
                let args = memory.read(GuestPtr(sp), 6)?;
                let op = crate::devices::Op::New {
                    reference: i16::from_be_bytes(args[4..6].try_into().unwrap()),
                    mode: i32::from_be_bytes(args[..4].try_into().unwrap()),
                };
                let next = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("NewGDevice stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 4)?;
                c.dar[15] = next;
                Ok((Call::Device(op), Reply::StackLong(GuestPtr(next))))
            }
            0xa9eb => {
                let selector = memory.u16(GuestPtr(sp))?;
                if [0, 2, 4, 6, 8, 10].contains(&(selector & 0xff)) {
                    let format = match selector & 0xff00 {
                        0 => crate::numerics::SourceFormat::Extended,
                        0x0800 => crate::numerics::SourceFormat::Double,
                        0x1000 => crate::numerics::SourceFormat::Single,
                        0x2000 => crate::numerics::SourceFormat::Integer,
                        0x2800 => crate::numerics::SourceFormat::Long,
                        _ => {
                            return Err(Error::Cpu(format!(
                                "unsupported SANE binary format {selector:04x}"
                            )))
                        }
                    };
                    let args = memory.read(GuestPtr(sp), 10)?;
                    let destination = GuestPtr(u32::from_be_bytes(args[2..6].try_into().unwrap()));
                    let source = GuestPtr(u32::from_be_bytes(args[6..10].try_into().unwrap()));
                    memory.read(source, format.bytes())?;
                    memory.read(destination, 10)?;
                    let comparison = [8, 10].contains(&(selector & 0xff));
                    if !comparison {
                        memory.validate_write(destination, 10)?;
                    }
                    c.dar[15] = sp
                        .checked_add(10)
                        .ok_or_else(|| Error::Cpu("SANE binary stack overflow".into()))?;
                    if comparison {
                        return Ok((
                            Call::NumericCompare {
                                source,
                                destination,
                                format,
                                signaling: selector & 0xff == 10,
                            },
                            Reply::SaneCompare,
                        ));
                    }
                    let op = match selector & 0xff {
                        0 => crate::numerics::Binary::Add,
                        2 => crate::numerics::Binary::Subtract,
                        4 => crate::numerics::Binary::Multiply,
                        _ => crate::numerics::Binary::Divide,
                    };
                    return Ok((
                        Call::NumericBinary {
                            source,
                            destination,
                            format,
                            op,
                        },
                        Reply::None,
                    ));
                }
                if [0x0d, 0x0f, 0x12, 0x14, 0x16].contains(&selector) {
                    let pointer =
                        GuestPtr(memory.u32(GuestPtr(sp.checked_add(2).ok_or_else(|| {
                            Error::Cpu("SANE square root stack overflow".into())
                        })?))?);
                    memory.read(pointer, 10)?;
                    memory.validate_write(pointer, 10)?;
                    c.dar[15] = sp
                        .checked_add(6)
                        .ok_or_else(|| Error::Cpu("SANE square root stack overflow".into()))?;
                    return Ok((
                        if selector == 0x0d || selector == 0x0f {
                            Call::SignExtended {
                                pointer,
                                absolute: selector == 0x0f,
                            }
                        } else if selector == 0x12 {
                            Call::SquareRootExtended(pointer)
                        } else {
                            Call::RoundIntegralExtended {
                                pointer,
                                truncate: selector == 0x16,
                            }
                        },
                        Reply::None,
                    ));
                }
                if [1, 3, 0x17, 0x19].contains(&selector) {
                    let pointer =
                        GuestPtr(memory.u32(GuestPtr(sp.checked_add(2).ok_or_else(|| {
                            Error::Cpu("SANE environment stack overflow".into())
                        })?))?);
                    let op = match selector {
                        1 => crate::numerics::Environment::Set,
                        3 => crate::numerics::Environment::Get,
                        0x17 => crate::numerics::Environment::Entry,
                        _ => crate::numerics::Environment::Exit,
                    };
                    if [3, 0x17].contains(&selector) {
                        memory.validate_write(pointer, 2)?;
                    } else {
                        memory.read(pointer, 2)?;
                    }
                    c.dar[15] = sp
                        .checked_add(6)
                        .ok_or_else(|| Error::Cpu("SANE environment stack overflow".into()))?;
                    return Ok((Call::NumericEnvironment { pointer, op }, Reply::None));
                }
                let args = memory.read(GuestPtr(sp), 10)?;
                let selector = u16::from_be_bytes(args[..2].try_into().unwrap());
                if ![
                    0x001c, 0x000e, 0x0810, 0x080e, 0x200e, 0x280e, 0x2010, 0x2810,
                ]
                .contains(&selector)
                {
                    return Err(Error::Cpu(format!(
                        "unsupported FP68K selector {selector:04x}"
                    )));
                }
                let destination = GuestPtr(u32::from_be_bytes(args[2..6].try_into().unwrap()));
                let source = GuestPtr(u32::from_be_bytes(args[6..10].try_into().unwrap()));
                memory.read(
                    source,
                    match selector {
                        0x080e => 8,
                        0x200e => 2,
                        0x280e => 4,
                        _ => 10,
                    },
                )?;
                memory.validate_write(
                    destination,
                    match selector {
                        0x001c | 0x2010 => 2,
                        0x2810 => 4,
                        0x0810 => 8,
                        _ => 10,
                    },
                )?;
                c.dar[15] = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("FP68K stack overflow".into()))?;
                Ok((
                    if selector == 0x001c {
                        Call::ClassifyExtended {
                            source,
                            destination,
                        }
                    } else if selector == 0x000e {
                        Call::CopyExtended {
                            source,
                            destination,
                        }
                    } else if [0x200e, 0x280e].contains(&selector) {
                        Call::IntegerToExtended {
                            source,
                            destination,
                            bits: if selector == 0x200e { 16 } else { 32 },
                        }
                    } else if [0x2010, 0x2810].contains(&selector) {
                        Call::ExtendedToInteger {
                            source,
                            destination,
                            bits: if selector == 0x2010 { 16 } else { 32 },
                        }
                    } else if selector == 0x080e {
                        Call::DoubleToExtended {
                            source,
                            destination,
                        }
                    } else {
                        Call::ExtendedToDouble {
                            source,
                            destination,
                        }
                    },
                    Reply::None,
                ))
            }
            0xa9ec => {
                let selector = memory.u16(GuestPtr(sp))?;
                if ![0x18, 0x1a, 0x1c, 0x1e].contains(&selector) {
                    return Err(Error::Cpu(format!(
                        "unsupported Elems68K selector {selector:04x}"
                    )));
                }
                let pointer = GuestPtr(
                    memory
                        .u32(GuestPtr(sp.checked_add(2).ok_or_else(|| {
                            Error::Cpu("Elems68K stack overflow".into())
                        })?))?,
                );
                memory.read(pointer, 10)?;
                memory.validate_write(pointer, 10)?;
                c.dar[15] = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("Elems68K stack overflow".into()))?;
                Ok((
                    if selector == 0x1e {
                        Call::AtanExtended(pointer)
                    } else {
                        Call::Elementary {
                            pointer,
                            op: match selector {
                                0x18 => crate::numerics::Unary::Sin,
                                0x1a => crate::numerics::Unary::Cos,
                                _ => crate::numerics::Unary::Tan,
                            },
                        }
                    },
                    Reply::None,
                ))
            }
            0xaa18 => {
                let id = memory.u16(GuestPtr(sp))? as i16;
                let next = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("GetCTable stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 4)?;
                c.dar[15] = next;
                Ok((
                    Call::Device(crate::devices::Op::GetTable(id)),
                    Reply::StackLong(GuestPtr(next)),
                ))
            }
            0xaa24 | 0xaa30 => {
                let device = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("DisposeGDevice stack overflow".into()))?;
                Ok((
                    Call::Device(if opcode == 0xaa24 {
                        crate::devices::Op::DisposeTable(device)
                    } else {
                        crate::devices::Op::Dispose(device)
                    }),
                    Reply::None,
                ))
            }
            0xaa27 => {
                let pointer = GuestPtr(memory.u32(GuestPtr(sp))?);
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("GetMaxDevice stack overflow".into()))?;
                memory.validate_write(GuestPtr(next), 4)?;
                c.dar[15] = next;
                Ok((
                    Call::Device(crate::devices::Op::Maximum(pointer)),
                    Reply::StackLong(GuestPtr(next)),
                ))
            }
            0xaa29 | 0xaa2a | 0xaa32 => {
                memory.validate_write(GuestPtr(sp), 4)?;
                let op = match opcode {
                    0xaa29 => crate::devices::Op::First,
                    0xaa2a => crate::devices::Op::Main,
                    _ => crate::devices::Op::Current,
                };
                Ok((Call::Device(op), Reply::StackLong(GuestPtr(sp))))
            }
            0xaa2b | 0xaa2c | 0xaa34 => {
                let count = match opcode {
                    0xaa2b => 4,
                    0xaa2c => 6,
                    _ => 8,
                };
                let args = memory.read(GuestPtr(sp), count)?;
                let long = |at| u32::from_be_bytes(args[at..at + 4].try_into().unwrap());
                let result = sp
                    .checked_add(count as u32)
                    .ok_or_else(|| Error::Cpu("device ABI stack overflow".into()))?;
                let (op, reply) = match opcode {
                    0xaa2b => {
                        memory.validate_write(GuestPtr(result), 4)?;
                        (
                            crate::devices::Op::Next(long(0)),
                            Reply::StackLong(GuestPtr(result)),
                        )
                    }
                    0xaa2c => {
                        memory.validate_write(GuestPtr(result), 1)?;
                        (
                            crate::devices::Op::Attribute {
                                device: long(2),
                                bit: i16::from_be_bytes(args[..2].try_into().unwrap()),
                            },
                            Reply::StackByte(GuestPtr(result)),
                        )
                    }
                    _ => {
                        memory.validate_write(GuestPtr(long(0)), 6)?;
                        (
                            crate::devices::Op::Index {
                                index: long(4) as i32,
                                destination: GuestPtr(long(0)),
                            },
                            Reply::None,
                        )
                    }
                };
                c.dar[15] = result;
                Ok((Call::Device(op), reply))
            }
            0xaa3c => {
                let sp = c.dar[15];
                let id = memory.u16(GuestPtr(sp))? as i16;
                let next = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("client ABI stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::Device(crate::devices::Op::Client(id)), Reply::None))
            }
            0xa02d => Ok((Call::SetApplLimit(c.dar[8]), Reply::M68kD0)),
            0xa036 => Ok((Call::MoreMasters, Reply::M68kD0)),
            0xa063 => Ok((Call::MaxApplZone, Reply::M68kD0)),
            0xa02e | 0xa22e => Ok((
                Call::MoveBytes {
                    source: GuestPtr(c.dar[8]),
                    destination: GuestPtr(c.dar[9]),
                    count: c.dar[0] as i32,
                },
                Reply::M68kD0,
            )),
            0xa97f => {
                let event = GuestPtr(memory.u32(GuestPtr(sp))?);
                let result = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("IsDialogEvent stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 1)?;
                c.dar[15] = result;
                Ok((
                    Call::Dialog(crate::dialogs::Op::IsEvent(event)),
                    Reply::StackByte(GuestPtr(result)),
                ))
            }
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
            0xa896 | 0xa897 => Ok((
                Call::PenVisibility(if opcode == 0xa896 { -1 } else { 1 }),
                Reply::None,
            )),
            0xa8cb => {
                memory.validate_write(GuestPtr(sp), 4)?;
                Ok((
                    Call::Polygon(crate::polygons::Op::Open),
                    Reply::StackLong(GuestPtr(sp)),
                ))
            }
            0xa8cc => Ok((Call::Polygon(crate::polygons::Op::Close), Reply::None)),
            0xa8c6..=0xa8c9 | 0xa8cd => {
                let handle = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("polygon stack overflow".into()))?;
                let op = if opcode == 0xa8cd {
                    crate::polygons::Op::Kill(handle)
                } else {
                    crate::polygons::Op::Draw {
                        handle,
                        verb: match opcode {
                            0xa8c6 => crate::polygons::Verb::Frame,
                            0xa8c7 => crate::polygons::Verb::Paint,
                            0xa8c8 => crate::polygons::Verb::Erase,
                            _ => crate::polygons::Verb::Invert,
                        },
                    }
                };
                Ok((Call::Polygon(op), Reply::None))
            }
            0xa8ca | 0xa8ce => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let handle = u32::from_be_bytes(args[4..8].try_into().unwrap());
                let op = if opcode == 0xa8ca {
                    crate::polygons::Op::Draw {
                        handle,
                        verb: crate::polygons::Verb::Fill(GuestPtr(u32::from_be_bytes(
                            args[..4].try_into().unwrap(),
                        ))),
                    }
                } else {
                    crate::polygons::Op::Offset {
                        handle,
                        dh: i16::from_be_bytes(args[2..4].try_into().unwrap()),
                        dv: i16::from_be_bytes(args[..2].try_into().unwrap()),
                    }
                };
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("polygon stack overflow".into()))?;
                Ok((Call::Polygon(op), Reply::None))
            }
            0xa89e => Ok((Call::PenNormal, Reply::None)),
            0xa933 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let source = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let handle = u32::from_be_bytes(args[4..8].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("append menu stack overflow".into()))?;
                Ok((
                    Call::Menu(crate::menus::Op::Append { handle, source }),
                    Reply::None,
                ))
            }
            0xa932 => {
                let handle = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("dispose menu stack overflow".into()))?;
                Ok((Call::Menu(crate::menus::Op::Dispose(handle)), Reply::None))
            }
            0xa931 => {
                let args = memory.read(GuestPtr(sp), 6)?;
                let title = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let id = i16::from_be_bytes(args[4..6].try_into().unwrap());
                let result = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("new menu stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                c.dar[15] = result;
                Ok((
                    Call::Menu(crate::menus::Op::New { id, title }),
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa9b1 => {
                let name = GuestPtr(memory.u32(GuestPtr(sp))?);
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("resource file stack overflow".into()))?;
                Ok((Call::CreateResourceFile(name), Reply::None))
            }
            0xa86a | 0xa86b => {
                let value = memory.u32(GuestPtr(sp))?;
                let result = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("word extraction stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::Word {
                        value,
                        high: opcode == 0xa86a,
                    },
                    Reply::StackWord(GuestPtr(result)),
                ))
            }
            0xa938 => {
                let id = memory.u16(GuestPtr(sp))? as i16;
                c.dar[15] = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("HiliteMenu stack overflow".into()))?;
                Ok((Call::HiliteMenu(id), Reply::None))
            }
            0xa93d => {
                let bytes = memory.read(GuestPtr(sp), 4)?;
                let point = [
                    i16::from_be_bytes(bytes[..2].try_into().unwrap()),
                    i16::from_be_bytes(bytes[2..].try_into().unwrap()),
                ];
                let result = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("MenuSelect stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                c.dar[15] = result;
                Ok((Call::MenuSelect(point), Reply::StackLong(GuestPtr(result))))
            }
            0xaa66 => {
                memory.validate_write(GuestPtr(sp), 4)?;
                Ok((Call::MenuChoice, Reply::StackLong(GuestPtr(sp))))
            }
            0xa939 | 0xa93a => {
                let args = memory.read(GuestPtr(sp), 6)?;
                let item = i16::from_be_bytes(args[..2].try_into().unwrap());
                let handle = u32::from_be_bytes(args[2..].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("menu enable stack overflow".into()))?;
                Ok((
                    Call::Menu(crate::menus::Op::Enable {
                        handle,
                        item,
                        enabled: opcode == 0xa939,
                    }),
                    Reply::None,
                ))
            }
            0xa946 | 0xa947 => {
                let args = memory.read(GuestPtr(sp), 10)?;
                let source = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let item = i16::from_be_bytes(args[4..6].try_into().unwrap());
                let handle = u32::from_be_bytes(args[6..10].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("menu text stack overflow".into()))?;
                Ok((
                    Call::Menu(if opcode == 0xa946 {
                        crate::menus::Op::ReadText {
                            handle,
                            item,
                            destination: source,
                        }
                    } else {
                        crate::menus::Op::Text {
                            handle,
                            item,
                            source,
                        }
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
                    let selector = memory.u16(GuestPtr(sp))?;
                    if selector == 9 {
                        let args = memory.read(GuestPtr(sp), 14)?;
                        let long = |at| u32::from_be_bytes(args[at..at + 4].try_into().unwrap());
                        let result = sp
                            .checked_add(14)
                            .ok_or_else(|| Error::Cpu("TEGetHeight stack overflow".into()))?;
                        memory.validate_write(GuestPtr(result), 4)?;
                        c.dar[15] = result;
                        return Ok((
                            Call::TextEdit(crate::textedit::Op::Height {
                                handle: long(2),
                                start: long(6) as i32,
                                end: long(10) as i32,
                            }),
                            Reply::StackLong(GuestPtr(result)),
                        ));
                    }
                    if selector != 4 {
                        return Err(Error::Cpu(format!(
                            "unsupported TextEdit selector {selector:#06x}"
                        )));
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
            0xa807 | 0xa801 => {
                let size = if opcode == 0xa807 { 14 } else { 6 };
                let bytes = memory.read(GuestPtr(sp), size)?;
                let long = |n| u32::from_be_bytes(bytes[n..n + 4].try_into().unwrap());
                let op = if opcode == 0xa807 {
                    let destination = GuestPtr(long(10));
                    memory.validate_write(destination, 4)?;
                    crate::sound::Op::New(crate::sound::NewChannel {
                        destination,
                        synth: i16::from_be_bytes(bytes[8..10].try_into().unwrap()),
                        init: long(4),
                        callback: long(0),
                    })
                } else {
                    crate::sound::Op::Dispose {
                        channel: long(2),
                        quiet_now: bytes[0] != 0,
                    }
                };
                let result = sp
                    .checked_add(size as u32)
                    .ok_or_else(|| Error::Cpu("sound channel stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((Call::SoundChannel(op), Reply::StackWord(GuestPtr(result))))
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
            0xa8da => Ok((
                Call::RegionRecording(crate::region_recording::Op::Open),
                Reply::None,
            )),
            0xa8db => {
                let handle = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("CloseRgn stack overflow".into()))?;
                Ok((
                    Call::RegionRecording(crate::region_recording::Op::Close(handle)),
                    Reply::None,
                ))
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

            0xa8e0 => {
                let args = memory.read(GuestPtr(sp), 8)?;
                let delta = u32::from_be_bytes(args[..4].try_into().unwrap());
                let handle = u32::from_be_bytes(args[4..].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("OffsetRgn stack overflow".into()))?;
                Ok((
                    Call::Region {
                        op: crate::regions::Op::Offset,
                        a: handle,
                        b: delta,
                    },
                    Reply::None,
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
            0xa8d9 | 0xa8dd | 0xa8dc | 0xa8df | 0xa879 | 0xa87a => {
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
                    0xa8dd => Op::Clear,
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
            0xa89b | 0xa876 | 0xa877 | 0xa878 => {
                let args = memory.read(GuestPtr(sp), 4)?;
                let height = i16::from_be_bytes(args[..2].try_into().unwrap());
                let width = i16::from_be_bytes(args[2..].try_into().unwrap());
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("QuickDraw stack overflow".into()))?;
                Ok((
                    if opcode == 0xa878 {
                        Call::SetOrigin {
                            h: width,
                            v: height,
                        }
                    } else if opcode == 0xa877 {
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
            0xa01c => Ok((Call::FreeMemory, Reply::M68kD0)),
            0xa01d | 0xa11d => Ok((
                Call::MaxMemory(None),
                Reply::M68kMaxMemory(opcode & 0x100 != 0),
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
            0xa013 => Ok((Call::FlushVolume(GuestPtr(c.dar[8])), Reply::M68kFileError)),
            0xa00d | 0xa20d | 0xa000 | 0xa200 | 0xa00a | 0xa20a | 0xa008 | 0xa208 | 0xa009
            | 0xa209 | 0xa001 | 0xa002 | 0xa003 | 0xa011 | 0xa012 | 0xa018 | 0xa044 | 0xa045 => {
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
                    0xa009 | 0xa209 => Op::Delete {
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
            0xa064 => Ok((Call::MoveHandleHigh(c.dar[8]), Reply::M68kError)),
            0xa069 => Ok((
                Call::HandleState {
                    handle: c.dar[8],
                    value: None,
                },
                Reply::M68kByte,
            )),
            0xa06a => Ok((
                Call::HandleState {
                    handle: c.dar[8],
                    value: Some(c.dar[0] as u8),
                },
                Reply::M68kError,
            )),
            0xa049 | 0xa04a | 0xa067 | 0xa068 => Ok((
                Call::HandleAttribute {
                    handle: c.dar[8],
                    resource: matches!(opcode, 0xa067 | 0xa068),
                    enabled: matches!(opcode, 0xa049 | 0xa067),
                },
                Reply::M68kError,
            )),
            0xa029 | 0xa02a => Ok((
                Call::SetHandleLock {
                    handle: c.dar[8],
                    locked: opcode == 0xa029,
                },
                Reply::M68kError,
            )),
            0xa9e1 => Ok((Call::CopyHandle(c.dar[8]), Reply::M68kHandle)),
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
            0xa817 => {
                let args = memory.read(GuestPtr(sp), 24)?;
                let pointer = |i| GuestPtr(u32::from_be_bytes(args[i..i + 4].try_into().unwrap()));
                let request = crate::quickdraw::CopyMask {
                    source: pointer(20),
                    mask: pointer(16),
                    destination: pointer(12),
                    source_rect: pointer(8),
                    mask_rect: pointer(4),
                    destination_rect: pointer(0),
                };
                c.dar[15] = sp
                    .checked_add(24)
                    .ok_or_else(|| Error::Cpu("CopyMask stack overflow".into()))?;
                Ok((Call::CopyMask(request), Reply::None))
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
            0xa9b8 | 0xa9b9 | 0xa9bb | 0xa9bc => {
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
                            0xa9bb => *b"ICON",
                            _ => *b"PAT ",
                        },
                        id,
                    },
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa9a8 => {
                let args = memory.read(GuestPtr(sp), 16)?;
                let pointer = |i| GuestPtr(u32::from_be_bytes(args[i..i + 4].try_into().unwrap()));
                let request = crate::process::ResourceInfo {
                    handle: pointer(12).0,
                    id: pointer(8),
                    kind: pointer(4),
                    name: pointer(0),
                };
                c.dar[15] = sp
                    .checked_add(16)
                    .ok_or_else(|| Error::Cpu("resource metadata stack overflow".into()))?;
                Ok((Call::ResourceInfo(request), Reply::None))
            }
            0xa9a0 | 0xa81f => {
                let args = memory.read(GuestPtr(sp), 10)?;
                let id = i16::from_be_bytes(args[..2].try_into().unwrap());
                let kind = args[2..6].try_into().unwrap();
                let result = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("resource ABI stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                c.dar[15] = result;
                Ok((
                    if opcode == 0xa81f {
                        Call::GetCurrentResource { kind, id }
                    } else {
                        Call::GetResource { kind, id }
                    },
                    Reply::StackLong(GuestPtr(result)),
                ))
            }
            0xa015 | 0xa215 => Ok((
                Call::WorkingDirectory {
                    pointer: GuestPtr(c.dar[8]),
                    op: crate::files::WorkingOp::Set {
                        hierarchical: opcode == 0xa215,
                    },
                },
                Reply::M68kFileError,
            )),
            0xa014 | 0xa214 => Ok((
                Call::WorkingDirectory {
                    pointer: GuestPtr(c.dar[8]),
                    op: crate::files::WorkingOp::Current {
                        hierarchical: opcode == 0xa214,
                    },
                },
                Reply::M68kFileError,
            )),
            0xa00c | 0xa20c => Ok((
                Call::FileInfo {
                    pointer: GuestPtr(c.dar[8]),
                    hierarchical: opcode == 0xa20c,
                },
                Reply::M68kFileError,
            )),
            0xa260 if matches!(c.dar[0] as u16, 1 | 2 | 7) => Ok((
                Call::WorkingDirectory {
                    pointer: GuestPtr(c.dar[8]),
                    op: match c.dar[0] as u16 {
                        1 => crate::files::WorkingOp::Open,
                        2 => crate::files::WorkingOp::Close,
                        _ => crate::files::WorkingOp::Info,
                    },
                },
                Reply::M68kFileError,
            )),
            0xa060 | 0xa260 if c.dar[0] as u16 == 0x1a => Ok((
                Call::FileIo {
                    pointer: GuestPtr(c.dar[8]),
                    op: crate::files::Op::Open {
                        kind: crate::files::Fork::Data,
                        hierarchical: opcode & 0x0200 != 0,
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
            0xaa68 if c.dar[0] as u16 == 0x0304 => {
                let b = memory.read(GuestPtr(sp), 8)?;
                let result = sp
                    .checked_add(6)
                    .ok_or_else(|| Error::Cpu("SetDialogDefaultItem stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                let call = Call::Dialog(crate::dialogs::Op::DefaultItem {
                    dialog: u32::from_be_bytes(b[2..6].try_into().unwrap()),
                    number: i16::from_be_bytes(b[..2].try_into().unwrap()),
                });
                c.dar[15] = result;
                Ok((call, Reply::StackWord(GuestPtr(result))))
            }
            0xaa14 | 0xaa15 => {
                let pointer = GuestPtr(memory.u32(GuestPtr(sp))?);
                if pointer.0 == 0 {
                    return Err(Error::Cpu("nil RGBColor".into()));
                }
                memory.read(pointer, 6)?;
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("RGBColor stack overflow".into()))?;
                c.dar[15] = next;
                Ok((
                    Call::RgbColor {
                        background: opcode == 0xaa15,
                        pointer,
                    },
                    Reply::None,
                ))
            }
            0xa98f | 0xa990 => {
                let b = memory.read(GuestPtr(sp), 8)?;
                let pointer = GuestPtr(u32::from_be_bytes(b[..4].try_into().unwrap()));
                let handle = u32::from_be_bytes(b[4..8].try_into().unwrap());
                if pointer.0 == 0 {
                    return Err(Error::Cpu("nil dialog text string".into()));
                }
                if opcode == 0xa990 {
                    memory.validate_write(pointer, 1)?;
                } else {
                    let n = memory.read(pointer, 1)?[0];
                    memory.read(pointer, usize::from(n) + 1)?;
                }
                let next = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("dialog text stack overflow".into()))?;
                c.dar[15] = next;
                Ok((
                    Call::Dialog(crate::dialogs::Op::Text {
                        handle,
                        pointer,
                        set: opcode == 0xa98f,
                    }),
                    Reply::None,
                ))
            }
            0xa981 => {
                let dialog = memory.u32(GuestPtr(sp))?;
                let next = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("DrawDialog stack overflow".into()))?;
                c.dar[15] = next;
                Ok((Call::Dialog(crate::dialogs::Op::Draw(dialog)), Reply::None))
            }
            0xa97c => {
                let b = memory.read(GuestPtr(sp), 14)?;
                let result = sp
                    .checked_add(10)
                    .ok_or_else(|| Error::Cpu("GetNewDialog stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                let op = crate::dialogs::Op::Resource {
                    id: i16::from_be_bytes(b[8..10].try_into().unwrap()),
                    storage: GuestPtr(u32::from_be_bytes(b[4..8].try_into().unwrap())),
                    behind: u32::from_be_bytes(b[..4].try_into().unwrap()),
                };
                c.dar[15] = result;
                Ok((Call::Dialog(op), Reply::StackLong(GuestPtr(result))))
            }
            0xa98d => {
                let b = memory.read(GuestPtr(sp), 18)?;
                let ptr = |i| GuestPtr(u32::from_be_bytes(b[i..i + 4].try_into().unwrap()));
                let op = crate::dialogs::Op::Item {
                    dialog: ptr(14).0,
                    number: i16::from_be_bytes(b[12..14].try_into().unwrap()),
                    kind: ptr(8),
                    handle: ptr(4),
                    bounds: ptr(0),
                };
                for (at, n) in [(8, 2), (4, 4), (0, 8)] {
                    memory.validate_write(ptr(at), n)?;
                }
                c.dar[15] = sp
                    .checked_add(18)
                    .ok_or_else(|| Error::Cpu("GetDialogItem stack overflow".into()))?;
                Ok((Call::Dialog(op), Reply::None))
            }
            0xa982 | 0xa983 => {
                let dialog = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("dialog disposal stack overflow".into()))?;
                Ok((
                    Call::Dialog(crate::dialogs::Op::Dispose {
                        dialog,
                        close: opcode == 0xa982,
                    }),
                    Reply::None,
                ))
            }
            0xa9a1 | 0xa820 => {
                let args = memory.read(GuestPtr(sp), 12)?;
                let name = GuestPtr(u32::from_be_bytes(args[..4].try_into().unwrap()));
                let kind = args[4..8].try_into().unwrap();
                let result = sp
                    .checked_add(8)
                    .ok_or_else(|| Error::Cpu("resource ABI stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
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
            0xa997 | 0xa9c4 => {
                let n = if opcode == 0xa9c4 { 8 } else { 4 };
                let bytes = memory.read(GuestPtr(sp), n + 2)?;
                let (name, volume, permission) = if n == 8 {
                    (
                        GuestPtr(u32::from_be_bytes(bytes[4..8].try_into().unwrap())),
                        i16::from_be_bytes(bytes[2..4].try_into().unwrap()),
                        bytes[0] as i8,
                    )
                } else {
                    (
                        GuestPtr(u32::from_be_bytes(bytes[..4].try_into().unwrap())),
                        0,
                        0,
                    )
                };
                if name.0 == 0 {
                    return Err(Error::Cpu("nil resource filename".into()));
                }
                let len = usize::from(memory.read(name, 1)?[0]);
                memory.read(name, len + 1)?;
                let result = sp
                    .checked_add(n as u32)
                    .ok_or_else(|| Error::Cpu("resource open stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::ResourceFile(crate::process::resources::Op::Open {
                        name,
                        volume,
                        permission,
                    }),
                    Reply::StackWord(GuestPtr(result)),
                ))
            }
            0xa999 => {
                let reference = memory.u16(GuestPtr(sp))? as i16;
                memory.validate_write(GuestPtr(0xa60), 2)?;
                let next = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("UpdateResFile stack overflow".into()))?;
                c.dar[15] = next;
                Ok((
                    Call::ResourceFile(crate::process::resources::Op::Update(reference)),
                    Reply::None,
                ))
            }
            0xa998 | 0xa99a => {
                let reference = memory.u16(GuestPtr(sp))? as i16;
                let next = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("resource file stack overflow".into()))?;
                c.dar[15] = next;
                Ok((
                    Call::ResourceFile(if opcode == 0xa998 {
                        crate::process::resources::Op::Use(reference)
                    } else {
                        crate::process::resources::Op::Close(reference)
                    }),
                    Reply::None,
                ))
            }
            0xa9a4 => {
                let h = memory.u32(GuestPtr(sp))?;
                let result = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("HomeResFile stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 2)?;
                c.dar[15] = result;
                Ok((
                    Call::ResourceFile(crate::process::resources::Op::Home(h)),
                    Reply::StackWord(GuestPtr(result)),
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
            Service::RgbColor(background) => Call::RgbColor {
                background,
                pointer: GuestPtr(c.gpr[3]),
            },
            Service::PortColor(background) => Call::PortColor {
                background,
                color: c.gpr[3],
            },
            Service::NewWindow | Service::NewColorWindow => {
                let request = crate::windows::NewWindow {
                    storage: GuestPtr(c.gpr[3]),
                    bounds: GuestPtr(c.gpr[4]),
                    title: GuestPtr(c.gpr[5]),
                    visible: c.gpr[6] as u8 != 0,
                    proc_id: c.gpr[7] as i16,
                    behind: c.gpr[8],
                    go_away: c.gpr[9] as u8 != 0,
                    ref_con: c.gpr[10],
                };
                if matches!(service, Service::NewColorWindow) {
                    Call::NewColorWindow(request)
                } else {
                    Call::NewWindow(request)
                }
            }
            Service::WindowRepaint(behind) => Call::WindowRepaint(crate::windows::Repaint {
                window: c.gpr[3],
                clobbered: c.gpr[4],
                behind,
            }),
            Service::WindowVisibility(behind) => Call::WindowVisibility(if behind {
                crate::windows::Visibility::Behind {
                    start: c.gpr[3],
                    clobbered: c.gpr[4],
                }
            } else {
                crate::windows::Visibility::Window(c.gpr[3])
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
            Service::EmptyRect => Call::EmptyRect(GuestPtr(c.gpr[3])),
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
            Service::ColorPort(action) => Call::ColorPort {
                action,
                port: GuestPtr(c.gpr[3]),
            },
            Service::MakeITable => Call::Color(crate::color::Op::Make {
                source: c.gpr[3],
                destination: c.gpr[4],
                resolution: c.gpr[5] as i16,
            }),
            Service::RgbToHsl => Call::Color(crate::color::Op::RgbToHsl {
                source: GuestPtr(c.gpr[3]),
                destination: GuestPtr(c.gpr[4]),
            }),
            Service::SetEntries => Call::Color(crate::color::Op::SetEntries {
                start: c.gpr[3] as i16,
                count: c.gpr[4] as i16,
                source: GuestPtr(c.gpr[5]),
            }),
            Service::GetCTSeed => Call::Color(crate::color::Op::Seed),
            Service::ReserveEntry => Call::Color(crate::color::Op::Reserve {
                index: c.gpr[3] as i16,
                reserve: c.gpr[4] as u8 != 0,
            }),
            Service::ProtectEntry => Call::Color(crate::color::Op::Protect {
                index: c.gpr[3] as i16,
                protect: c.gpr[4] as u8 != 0,
            }),
            Service::QDError => Call::Color(crate::color::Op::Error),
            Service::HasDepth | Service::SetDepth => Call::Device(crate::devices::Op::Depth {
                device: c.gpr[3],
                depth: c.gpr[4] as i16,
                which_flags: c.gpr[5] as u16,
                flags: c.gpr[6] as u16,
                set: service == Service::SetDepth,
            }),
            Service::Device(op) => Call::Device(op),
            Service::SetClientID => Call::Device(crate::devices::Op::Client(c.gpr[3] as i16)),
            Service::SetDevice => Call::Device(crate::devices::Op::Select(c.gpr[3])),
            Service::GetColorTable => Call::Device(crate::devices::Op::GetTable(c.gpr[3] as i16)),
            Service::DisposeColorTable => Call::Device(crate::devices::Op::DisposeTable(c.gpr[3])),
            Service::NewDevice => Call::Device(crate::devices::Op::New {
                reference: c.gpr[3] as i16,
                mode: c.gpr[4] as i32,
            }),
            Service::DisposeDevice => Call::Device(crate::devices::Op::Dispose(c.gpr[3])),
            Service::MaxDevice => Call::Device(crate::devices::Op::Maximum(GuestPtr(c.gpr[3]))),
            Service::NextDevice => Call::Device(crate::devices::Op::Next(c.gpr[3])),
            Service::SetDeviceAttribute => Call::Device(crate::devices::Op::SetAttribute {
                device: c.gpr[3],
                bit: c.gpr[4] as i16,
                value: c.gpr[5] as u8 != 0,
            }),
            Service::DeviceAttribute => Call::Device(crate::devices::Op::Attribute {
                device: c.gpr[3],
                bit: c.gpr[4] as i16,
            }),
            Service::Index2Color => Call::Device(crate::devices::Op::Index {
                index: c.gpr[3] as i32,
                destination: GuestPtr(c.gpr[4]),
            }),
            Service::MoreMasters => Call::MoreMasters,
            Service::MaxApplZone => Call::MaxApplZone,
            Service::SetApplLimit => Call::SetApplLimit(c.gpr[3]),
            Service::GetApplLimit => Call::GetApplLimit,
            Service::BlockMove => Call::MoveBytes {
                source: GuestPtr(c.gpr[3]),
                destination: GuestPtr(c.gpr[4]),
                count: c.gpr[5] as i32,
            },
            Service::CopyMask => Call::CopyMask(crate::quickdraw::CopyMask {
                source: GuestPtr(c.gpr[3]),
                mask: GuestPtr(c.gpr[4]),
                destination: GuestPtr(c.gpr[5]),
                source_rect: GuestPtr(c.gpr[6]),
                mask_rect: GuestPtr(c.gpr[7]),
                destination_rect: GuestPtr(c.gpr[8]),
            }),
            Service::VblHeader => Call::RetraceHeader,
            Service::Retrace { remove, slot } => Call::Retrace(crate::retrace::Op {
                task: GuestPtr(c.gpr[3]),
                slot: slot.then_some(c.gpr[4] as i16),
                remove,
            }),
            Service::Timer(action) => Call::Timer(crate::timers::Op {
                task: GuestPtr(c.gpr[3]),
                action: match action {
                    0 => crate::timers::Action::Insert,
                    1 => crate::timers::Action::Prime(c.gpr[4] as i32),
                    _ => crate::timers::Action::Remove,
                },
            }),
            Service::SecondsToDate => Call::SecondsToDate {
                seconds: c.gpr[3],
                destination: GuestPtr(c.gpr[4]),
            },
            Service::GetDateTime | Service::ReadDateTime => Call::ReadDateTime(GuestPtr(c.gpr[3])),
            Service::NumToString => Call::Decimal(crate::decimal::Op::Format {
                number: c.gpr[3] as i32,
                destination: GuestPtr(c.gpr[4]),
            }),
            Service::StringToNum => Call::Decimal(crate::decimal::Op::Parse {
                source: GuestPtr(c.gpr[3]),
                destination: Some(GuestPtr(c.gpr[4])),
            }),
            Service::GetDAString => Call::Dialog(crate::dialogs::Op::Slot(c.gpr[3] as i16)),
            Service::IsDialogEvent => Call::Dialog(crate::dialogs::Op::IsEvent(GuestPtr(c.gpr[3]))),
            Service::DialogItemText(set) => Call::Dialog(crate::dialogs::Op::Text {
                handle: c.gpr[3],
                pointer: GuestPtr(c.gpr[4]),
                set,
            }),
            Service::DrawDialog => Call::Dialog(crate::dialogs::Op::Draw(c.gpr[3])),
            Service::SetDialogDefaultItem => Call::Dialog(crate::dialogs::Op::DefaultItem {
                dialog: c.gpr[3],
                number: c.gpr[4] as i16,
            }),
            Service::GetNewDialog => Call::Dialog(crate::dialogs::Op::Resource {
                id: c.gpr[3] as i16,
                storage: GuestPtr(c.gpr[4]),
                behind: c.gpr[5],
            }),
            Service::GetDialogItem => Call::Dialog(crate::dialogs::Op::Item {
                dialog: c.gpr[3],
                number: c.gpr[4] as i16,
                kind: GuestPtr(c.gpr[5]),
                handle: GuestPtr(c.gpr[6]),
                bounds: GuestPtr(c.gpr[7]),
            }),
            Service::DisposeDialog(close) => Call::Dialog(crate::dialogs::Op::Dispose {
                dialog: c.gpr[3],
                close,
            }),
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
            Service::AppendMenu => Call::Menu(crate::menus::Op::Append {
                handle: c.gpr[3],
                source: GuestPtr(c.gpr[4]),
            }),
            Service::DisposeMenu => Call::Menu(crate::menus::Op::Dispose(c.gpr[3])),
            Service::NewMenu => Call::Menu(crate::menus::Op::New {
                id: c.gpr[3] as i16,
                title: GuestPtr(c.gpr[4]),
            }),
            Service::GetMenuItemText => Call::Menu(crate::menus::Op::ReadText {
                handle: c.gpr[3],
                item: c.gpr[4] as i16,
                destination: GuestPtr(c.gpr[5]),
            }),
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
            Service::MenuSelect => Call::MenuSelect([(c.gpr[3] >> 16) as i16, c.gpr[3] as i16]),
            Service::HiliteMenu => Call::HiliteMenu(c.gpr[3] as i16),
            Service::Word(high) => Call::Word {
                value: c.gpr[3],
                high,
            },
            Service::MenuChoice => Call::MenuChoice,
            Service::EnableItem(enabled) => Call::Menu(crate::menus::Op::Enable {
                handle: c.gpr[3],
                item: c.gpr[4] as i16,
                enabled,
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
            Service::TEGetHeight => Call::TextEdit(crate::textedit::Op::Height {
                handle: c.gpr[5],
                start: c.gpr[4] as i32,
                end: c.gpr[3] as i32,
            }),
            Service::TEGetText => Call::TextEdit(crate::textedit::Op::GetText(c.gpr[3])),
            Service::TEGetStyleHandle => Call::TextEdit(crate::textedit::Op::GetStyle(c.gpr[3])),
            Service::TEDispose => Call::TextEdit(crate::textedit::Op::Dispose(c.gpr[3])),
            Service::TrackControl => Call::TrackControl(crate::controls::TrackControl {
                handle: c.gpr[3],
                point: [(c.gpr[4] >> 16) as i16, c.gpr[4] as i16],
                action: c.gpr[5],
            }),
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
            Service::ControlGeometry(resize) => Call::Control(crate::controls::Op::Geometry {
                handle: c.gpr[3],
                change: if resize {
                    crate::controls::Geometry::Size {
                        width: c.gpr[4] as i16,
                        height: c.gpr[5] as i16,
                    }
                } else {
                    crate::controls::Geometry::Move {
                        h: c.gpr[4] as i16,
                        v: c.gpr[5] as i16,
                    }
                },
            }),
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
            Service::GetNewWindow | Service::GetNewColorWindow => {
                Call::ResourceWindow(crate::windows::ResourceWindow {
                    id: c.gpr[3] as i16,
                    storage: GuestPtr(c.gpr[4]),
                    behind: c.gpr[5],
                    color: service == Service::GetNewColorWindow,
                })
            }
            Service::Random => Call::Random,
            Service::FixedMultiply => Call::FixedMultiply {
                left: c.gpr[3] as i32,
                right: c.gpr[4] as i32,
            },
            Service::LongToFixed => Call::LongToFixed(c.gpr[3] as i32),
            Service::FixedRatio => Call::FixedRatio {
                numerator: c.gpr[3] as i16,
                denominator: c.gpr[4] as i16,
            },
            Service::Logical(op) => Call::Logical {
                op,
                left: c.gpr[3],
                right: if op == crate::integer::Logic::Not {
                    0
                } else {
                    c.gpr[4]
                },
            },
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
                b: if op == crate::regions::Op::Offset {
                    ((c.gpr[5] as u16 as u32) << 16) | (c.gpr[4] as u16 as u32)
                } else {
                    c.gpr[4]
                },
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
            Service::SetOrigin => Call::SetOrigin {
                h: c.gpr[3] as i16,
                v: c.gpr[4] as i16,
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
            Service::RegionRecording(close) => Call::RegionRecording(if close {
                crate::region_recording::Op::Close(c.gpr[3])
            } else {
                crate::region_recording::Op::Open
            }),
            Service::Polygon(op) => Call::Polygon(op),
            Service::PolygonDraw(verb) => Call::Polygon(crate::polygons::Op::Draw {
                handle: c.gpr[3],
                verb,
            }),
            Service::FillPoly => Call::Polygon(crate::polygons::Op::Draw {
                handle: c.gpr[3],
                verb: crate::polygons::Verb::Fill(GuestPtr(c.gpr[4])),
            }),
            Service::KillPoly => Call::Polygon(crate::polygons::Op::Kill(c.gpr[3])),
            Service::OffsetPoly => Call::Polygon(crate::polygons::Op::Offset {
                handle: c.gpr[3],
                dh: c.gpr[4] as i16,
                dv: c.gpr[5] as i16,
            }),
            Service::PenVisibility(delta) => Call::PenVisibility(delta),
            Service::PenNormal => Call::PenNormal,
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
            Service::MoveHHi => Call::MoveHandleHigh(c.gpr[3]),
            Service::HGetState => Call::HandleState {
                handle: c.gpr[3],
                value: None,
            },
            Service::HSetState => Call::HandleState {
                handle: c.gpr[3],
                value: Some(c.gpr[4] as u8),
            },
            Service::HandleAttribute { resource, enabled } => Call::HandleAttribute {
                handle: c.gpr[3],
                resource,
                enabled,
            },
            Service::HLock | Service::HUnlock => Call::SetHandleLock {
                handle: c.gpr[3],
                locked: service == Service::HLock,
            },
            Service::HandToHand => {
                let output = GuestPtr(c.gpr[3]);
                memory.validate_write(output, 4)?;
                let source = memory.u32(output)?;
                return Ok((Call::CopyHandle(source), Reply::PpcHandleCopy(output)));
            }
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
            Service::SFGetFile => {
                let request = crate::standard_file::GetFile {
                    point: [(c.gpr[3] >> 16) as i16, c.gpr[3] as i16],
                    filter: c.gpr[5],
                    count: c.gpr[6] as i16,
                    types: GuestPtr(c.gpr[7]),
                    hook: c.gpr[8],
                    reply: GuestPtr(c.gpr[9]),
                };
                request.validate(memory)?;
                Call::StandardGetFile(request)
            }
            Service::SFPutFile => Call::StandardPutFile(crate::standard_file::PutFile {
                point: [(c.gpr[3] >> 16) as i16, c.gpr[3] as i16],
                prompt: GuestPtr(c.gpr[4]),
                name: GuestPtr(c.gpr[5]),
                hook: c.gpr[6],
                reply: GuestPtr(c.gpr[7]),
            }),
            Service::GetWMgrPort => Call::GetWMgrPort(GuestPtr(c.gpr[3])),
            Service::GetIcon => Call::GetResource {
                kind: *b"ICON",
                id: c.gpr[3] as i16,
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
            Service::CreateResFile => Call::CreateResourceFile(GuestPtr(c.gpr[3])),
            Service::GetResInfo => Call::ResourceInfo(crate::process::ResourceInfo {
                handle: c.gpr[3],
                id: GuestPtr(c.gpr[4]),
                kind: GuestPtr(c.gpr[5]),
                name: GuestPtr(c.gpr[6]),
            }),
            Service::Get1Resource => Call::GetCurrentResource {
                kind: c.gpr[3].to_be_bytes(),
                id: c.gpr[4] as i16,
            },
            Service::GetResource => Call::GetResource {
                kind: c.gpr[3].to_be_bytes(),
                id: c.gpr[4] as i16,
            },
            Service::SndSoundManagerVersion => Call::SoundVersion,
            Service::SndChannelStatus => Call::SoundStatus(crate::sound::Status {
                channel: Some(c.gpr[3]),
                length: c.gpr[4] as i16,
                destination: GuestPtr(c.gpr[5]),
            }),
            Service::SndManagerStatus => Call::SoundStatus(crate::sound::Status {
                channel: None,
                length: c.gpr[3] as i16,
                destination: GuestPtr(c.gpr[4]),
            }),
            Service::SndStartFilePlay => Call::SoundStartFile(crate::sound::StartFilePlay {
                channel: c.gpr[3],
                file: c.gpr[4] as i16,
                resource: c.gpr[5] as i16,
                buffer_size: c.gpr[6] as i32,
                buffer: GuestPtr(c.gpr[7]),
                selection: GuestPtr(c.gpr[8]),
                completion: c.gpr[9],
                asynchronous: c.gpr[10] as u8 != 0,
            }),
            Service::SndPlay => Call::SoundPlay {
                channel: c.gpr[3],
                handle: c.gpr[4],
                asynchronous: c.gpr[5] as u8 != 0,
            },
            Service::SndDoCommand => Call::SoundQueue {
                channel: c.gpr[3],
                command: GuestPtr(c.gpr[4]),
                no_wait: c.gpr[5] as u8 != 0,
            },
            Service::SndDoImmediate => Call::SoundImmediate {
                channel: c.gpr[3],
                command: GuestPtr(c.gpr[4]),
            },
            Service::SndNewChannel => {
                let destination = GuestPtr(c.gpr[3]);
                memory.validate_write(destination, 4)?;
                Call::SoundChannel(crate::sound::Op::New(crate::sound::NewChannel {
                    destination,
                    synth: c.gpr[4] as i16,
                    init: c.gpr[5],
                    callback: c.gpr[6],
                }))
            }
            Service::SndDisposeChannel => Call::SoundChannel(crate::sound::Op::Dispose {
                channel: c.gpr[3],
                quiet_now: c.gpr[4] as u8 != 0,
            }),
            Service::SetSoundVol => Call::SoundVolume(c.gpr[3] as i16),
            Service::GetSoundVol => Call::GetSoundVolume(GuestPtr(c.gpr[3])),
            Service::PBControlSync => Call::DeviceControl(GuestPtr(c.gpr[3])),
            Service::StripAddress => Call::StripAddress(c.gpr[3]),
            Service::FreeMemory => Call::FreeMemory,
            Service::MaxMemory => {
                let grow = GuestPtr(c.gpr[3]);
                if grow.0 == 0 {
                    return Err(Error::Cpu("nil MaxMem growth output".into()));
                }
                memory.validate_write(grow, 4)?;
                Call::MaxMemory(Some(grow))
            }
            Service::MemError => Call::MemError,
            Service::ResError => Call::ResError,
            Service::CurResFile => Call::CurResFile,
            Service::OpenSpecResource => {
                Call::ResourceFile(crate::process::resources::Op::OpenSpec {
                    spec: GuestPtr(c.gpr[3]),
                    permission: c.gpr[4] as i8,
                })
            }
            Service::OpenResourceFile(permission) => {
                Call::ResourceFile(crate::process::resources::Op::Open {
                    name: GuestPtr(c.gpr[3]),
                    volume: if permission { c.gpr[4] as i16 } else { 0 },
                    permission: if permission { c.gpr[5] as i8 } else { 0 },
                })
            }
            Service::ResourceFile(action) => Call::ResourceFile(match action {
                crate::process::resources::Action::Update => {
                    crate::process::resources::Op::Update(c.gpr[3] as i16)
                }
                crate::process::resources::Action::Use => {
                    crate::process::resources::Op::Use(c.gpr[3] as i16)
                }
                crate::process::resources::Action::Close => {
                    crate::process::resources::Op::Close(c.gpr[3] as i16)
                }
                crate::process::resources::Action::Home => {
                    crate::process::resources::Op::Home(c.gpr[3])
                }
            }),
            Service::GetNamedResource | Service::Get1NamedResource => Call::GetNamedResource {
                kind: c.gpr[3].to_be_bytes(),
                name: GuestPtr(c.gpr[4]),
                current_only: service == Service::Get1NamedResource,
            },
            Service::FlushVolume => Call::FlushVolume(GuestPtr(c.gpr[3])),
            Service::FlushVolumeWithAsync => {
                if c.gpr[4] as u8 != 0 {
                    return Err(Error::Cpu(
                        "asynchronous volume flush is unsupported".into(),
                    ));
                }
                Call::FlushVolume(GuestPtr(c.gpr[3]))
            }
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
            Service::WorkingDirectory(op) => Call::WorkingDirectory {
                pointer: GuestPtr(c.gpr[3]),
                op,
            },
            Service::AddResource => Call::AddResource(crate::process::resources::AddResource {
                handle: c.gpr[3],
                kind: c.gpr[4].to_be_bytes(),
                id: c.gpr[5] as i16,
                name: GuestPtr(c.gpr[6]),
            }),
            Service::CreateSpecResource => {
                Call::CreateSpecResource(crate::files::CreateSpecResource {
                    spec: GuestPtr(c.gpr[3]),
                    creator: c.gpr[4].to_be_bytes(),
                    kind: c.gpr[5].to_be_bytes(),
                    script: c.gpr[6] as i16,
                })
            }
            Service::GetSpecInfo => Call::GetSpecInfo(crate::files::GetSpecInfo {
                spec: GuestPtr(c.gpr[3]),
                destination: GuestPtr(c.gpr[4]),
            }),
            Service::DeleteSpec => Call::DeleteSpec(GuestPtr(c.gpr[3])),
            Service::MakeSpec => Call::MakeSpec(crate::files::MakeSpec {
                volume: c.gpr[3] as i16,
                directory: c.gpr[4],
                name: GuestPtr(c.gpr[5]),
                destination: GuestPtr(c.gpr[6]),
            }),
            Service::ResolveAliasFile => Call::ResolveAliasFile(crate::files::ResolveAliasFile {
                spec: GuestPtr(c.gpr[3]),
                chains: c.gpr[4] as u8 != 0,
                folder: GuestPtr(c.gpr[5]),
                aliased: GuestPtr(c.gpr[6]),
            }),
            Service::FcbInfo => Call::FcbInfo(GuestPtr(c.gpr[3])),
            Service::CatalogInfo => Call::CatalogInfo(GuestPtr(c.gpr[3])),
            Service::FileInfo(hierarchical) => Call::FileInfo {
                pointer: GuestPtr(c.gpr[3]),
                hierarchical,
            },
            Service::CreateDirectory => Call::CreateDirectory(GuestPtr(c.gpr[3])),
        };
        // NumVersion is a structure: the PowerPC ABI passes its return buffer in r3.
        if service == Service::SndSoundManagerVersion {
            let output = GuestPtr(c.gpr[3]);
            memory.validate_write(output, 4)?;
            return Ok((call, Reply::StackLong(output)));
        }
        if service == Service::Gestalt {
            return Ok((call, Reply::PpcGestalt(GuestPtr(c.gpr[4]))));
        }
        Ok((
            call,
            if matches!(service, Service::Region(op) if !matches!(op, crate::regions::Op::New | crate::regions::Op::Contains | crate::regions::Op::Intersects | crate::regions::Op::Empty | crate::regions::Op::Equal))
                || matches!(
                    service,
                    Service::RgbToHsl
                        | Service::SetEntries
                        | Service::ReserveEntry
                        | Service::ProtectEntry
                        | Service::SetClientID
                        | Service::SetDevice
                        | Service::GetDialogItem
                        | Service::DrawDialog
                        | Service::DialogItemText(_)
                        | Service::DisposeDialog(_)
                        | Service::MakeITable
                        | Service::SetDeviceAttribute
                        | Service::AddResource
                        | Service::CreateSpecResource
                        | Service::SecondsToDate
                        | Service::GetDateTime
                        | Service::GetResInfo
                        | Service::CopyMask
                        | Service::NumToString
                        | Service::StringToNum
                        | Service::GetMouse
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
                        | Service::ControlGeometry(_)
                        | Service::ControlSetValue
                        | Service::ControlSetReference
                        | Service::ControlTitle
                        | Service::ControlSetTitle
                        | Service::SetRect
                        | Service::OffsetRect
                        | Service::InsetRect
                        | Service::MoreMasters
                        | Service::ColorPort(_)
                        | Service::Index2Color
                        | Service::SetApplLimit
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
                        | Service::DisposeMenu
                        | Service::CreateResFile
                        | Service::AppendMenu
                        | Service::AppendResMenu
                        | Service::SetItemMark
                        | Service::SetItemCommand
                        | Service::SetMenuItemText
                        | Service::GetMenuItemText
                        | Service::HiliteMenu
                        | Service::EnableItem(_)
                        | Service::CheckItem
                        | Service::Menu(
                            crate::menus::Op::Init
                                | crate::menus::Op::Draw
                                | crate::menus::Op::Clear
                        )
                        | Service::SFPutFile
                        | Service::SFGetFile
                        | Service::GetWMgrPort
                        | Service::InitGraf
                        | Service::PortColor(_)
                        | Service::RgbColor(_)
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
                        | Service::SetOrigin
                        | Service::MovePortTo
                        | Service::PortSize
                        | Service::PenSize
                        | Service::PenMode
                        | Service::PenNormal
                        | Service::PenVisibility(_)
                        | Service::WindowRepaint(_)
                        | Service::WindowVisibility(_)
                        | Service::RegionRecording(_)
                        | Service::Polygon(crate::polygons::Op::Close)
                        | Service::PolygonDraw(_)
                        | Service::KillPoly
                        | Service::OffsetPoly
                        | Service::FillPoly
                        | Service::ExitToShell
                        | Service::SetPointerSize
                        | Service::DisposePointer
                        | Service::Timer(_)
                        | Service::SetHandleSize
                        | Service::DetachResource
                        | Service::ReleaseResource
                        | Service::ResourceFile(
                            crate::process::resources::Action::Use
                                | crate::process::resources::Action::Update
                                | crate::process::resources::Action::Close
                        )
                        | Service::SetSoundVol
                        | Service::GetSoundVol
                        | Service::MoveHHi
                        | Service::HSetState
                        | Service::HandleAttribute { .. }
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
            Self::SaneCompare => {
                let bits = match value {
                    0 => 0x19,
                    1 => 4,
                    2 => 0,
                    3 => 2,
                    _ => return Err(Error::Cpu("invalid SANE relation result".into())),
                };
                let Cpu::M68k(c) = cpu else {
                    return Err(Error::Cpu(
                        "native SANE comparison reply on another CPU".into(),
                    ));
                };
                c.set_sr((c.get_sr() & !0x1f) | bits);
            }
            Self::M68kMaxMemory(grow) => {
                let Cpu::M68k(c) = cpu else {
                    return Err(Error::Cpu("native MaxMem reply on PPC".into()));
                };
                c.dar[0] = value;
                if grow {
                    c.dar[8] = 0;
                }
                let flags = if value as u16 == 0 {
                    4
                } else if value & 0x8000 != 0 {
                    8
                } else {
                    0
                };
                c.set_sr((c.get_sr() & !0xf) | flags);
            }
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
            Self::PpcHandleCopy(output) => {
                let Cpu::PowerPc(c) = cpu else {
                    return Err(Error::Cpu("PPC handle copy result on 68K".into()));
                };
                memory.write(output, &value.to_be_bytes())?;
                c.gpr[3] = error as i32 as u32;
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
            Self::M68kByte => match cpu {
                Cpu::M68k(c) => c.dar[0] = (c.dar[0] & 0xffff_ff00) | (value & 0xff),
                _ => return Err(Error::Cpu("68K byte result on PPC".into())),
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
