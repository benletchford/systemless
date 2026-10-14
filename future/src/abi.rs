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
    Random,
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
    GetHandleSize,
    SetHandleSize,
    GetResource,
    GetPattern,
    SetSoundVol,
    GetSoundVol,
    PBControlSync,
    StripAddress,
    MemError,
    ResError,
    CurResFile,
    GetNamedResource,
    Get1NamedResource,
    FcbInfo,
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
            b"NewWindow" => Some(Service::NewWindow),
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
            b"Random" => Some(Service::Random),
            b"FindSymbol" => Some(Service::FindSymbol),
            b"CountSymbols" => Some(Service::CountSymbols),
            b"GetIndSymbol" => Some(Service::GetIndSymbol),
            b"PtInRgn" => Some(Service::Region(crate::regions::Op::Contains)),
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
            b"PaintRect" => Some(Service::Port(crate::quickdraw::PortOp::PaintRect)),
            b"PenPat" => Some(Service::Port(crate::quickdraw::PortOp::PenPat)),
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
            b"SetSoundVol" => Some(Service::SetSoundVol),
            b"GetSoundVol" => Some(Service::GetSoundVol),
            b"PBControlSync" => Some(Service::PBControlSync),
            b"StripAddress" => Some(Service::StripAddress),
            b"HLock" => Some(Service::HLock),
            b"HUnlock" => Some(Service::HUnlock),
            b"GetPattern" => Some(Service::GetPattern),
            b"GetResource" => Some(Service::GetResource),
            b"MemError" => Some(Service::MemError),
            b"PBGetFCBInfoSync" => Some(Service::FcbInfo),
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
            0xa92d | 0xa914 => {
                let pointer = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("window stack overflow".into()))?;
                Ok((
                    Call::Window {
                        op: if opcode == 0xa92d {
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

            0xa8e8 | 0xa8e2 => {
                use crate::regions::Op;
                let pair = opcode == 0xa8e8;
                let n = if pair { 8 } else { 4 };
                let args = memory.read(GuestPtr(sp), n + 2)?;
                let h = u32::from_be_bytes(args[..4].try_into().unwrap());
                let (op, a, b) = if pair {
                    (
                        Op::Contains,
                        u32::from_be_bytes(args[4..8].try_into().unwrap()),
                        h,
                    )
                } else {
                    (Op::Empty, h, 0)
                };
                let result = sp
                    .checked_add(n as u32)
                    .ok_or_else(|| Error::Cpu("region stack overflow".into()))?;
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
            0xa9a3 => {
                let handle = memory.u32(GuestPtr(sp))?;
                c.dar[15] = sp
                    .checked_add(4)
                    .ok_or_else(|| Error::Cpu("resource ABI stack overflow".into()))?;
                Ok((Call::ReleaseResource(handle), Reply::None))
            }
            0xa9b8 => {
                let id = memory.u16(GuestPtr(sp))? as i16;
                let result = sp
                    .checked_add(2)
                    .ok_or_else(|| Error::Cpu("resource ABI stack overflow".into()))?;
                memory.validate_write(GuestPtr(result), 4)?;
                c.dar[15] = result;
                Ok((
                    Call::GetResource { kind: *b"PAT ", id },
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
    pub fn import(service: Service, c: &ppc::PpcCpu) -> (Call, Reply) {
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
            Service::FcbInfo => Call::FcbInfo(GuestPtr(c.gpr[3])),
        };
        (
            call,
            if matches!(service, Service::Region(op) if !matches!(op, crate::regions::Op::New | crate::regions::Op::Contains | crate::regions::Op::Empty))
                || matches!(
                    service,
                    Service::InitGraf
                        | Service::PortColor(_)
                        | Service::Window(
                            crate::windows::Op::Init
                                | crate::windows::Op::Close
                                | crate::windows::Op::Dispose
                        )
                        | Service::InitFonts
                        | Service::FontInfo
                        | Service::TextState(_)
                        | Service::RoundRect(_)
                        | Service::CombineRegions(_)
                        | Service::Port(_)
                        | Service::PenMode
                        | Service::ExitToShell
                        | Service::SetPointerSize
                        | Service::DisposePointer
                        | Service::SetHandleSize
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
        )
    }
}
impl Reply {
    pub fn apply(self, cpu: &mut Cpu, memory: &mut Memory, value: u32, error: i16) -> Result<()> {
        match self {
            Self::None => {}
            Self::StackByte(p) => memory.write(p, &[value as u8])?,
            Self::StackLong(p) => memory.write(p, &value.to_be_bytes())?,
            Self::StackWord(p) => memory.write(p, &(value as u16).to_be_bytes())?,
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
