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
    InitGraf,
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
    GetOSTrapAddress,
    GetToolboxTrapAddress,
    NGetTrapAddress,
    ExitToShell,
    NewHandle,
    NewHandleClear,
    DisposeHandle,
    GetHandleSize,
    SetHandleSize,
    GetResource,
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
            b"PtInRgn" => Some(Service::Region(crate::regions::Op::Contains)),
            b"EmptyRgn" => Some(Service::Region(crate::regions::Op::Empty)),
            b"NewRgn" => Some(Service::Region(crate::regions::Op::New)),
            b"DisposeRgn" => Some(Service::Region(crate::regions::Op::Dispose)),
            b"CopyRgn" => Some(Service::Region(crate::regions::Op::Copy)),
            b"RectRgn" => Some(Service::Region(crate::regions::Op::Rect)),
            b"GetClip" => Some(Service::Region(crate::regions::Op::GetClip)),
            b"SetClip" => Some(Service::Region(crate::regions::Op::SetClip)),
            b"InitGraf" => Some(Service::InitGraf),
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
            b"GetOSTrapAddress" => Some(Service::GetOSTrapAddress),
            b"GetToolboxTrapAddress" | b"GetToolTrapAddress" => {
                Some(Service::GetToolboxTrapAddress)
            }
            b"NGetTrapAddress" => Some(Service::NGetTrapAddress),
            b"TickCount" => Some(Service::TickCount),
            b"ExitToShell" => Some(Service::ExitToShell),
            b"NewHandle" => Some(Service::NewHandle),
            b"NewHandleClear" => Some(Service::NewHandleClear),
            b"DisposeHandle" => Some(Service::DisposeHandle),
            b"SetHandleSize" => Some(Service::SetHandleSize),
            b"GetHandleSize" => Some(Service::GetHandleSize),
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

            0xa86d | 0xa86e | 0xa86f | 0xa87d | 0xa873 | 0xa874 | 0xa875 | 0xa87b | 0xa8a2
            | 0xa89d => {
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
            0xa122 | 0xa022 | 0xa322 => Ok((
                Call::NewHandle {
                    size: c.dar[0],
                    clear: opcode & 0x200 != 0,
                },
                Reply::M68kHandle,
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
            Service::Region(op) => Call::Region {
                op,
                a: c.gpr[3],
                b: c.gpr[4],
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
            Service::GetOSTrapAddress
            | Service::GetToolboxTrapAddress
            | Service::NGetTrapAddress => Call::TrapAddress {
                number: c.gpr[3] as u16,
                toolbox: service == Service::GetToolboxTrapAddress
                    || (service == Service::NGetTrapAddress && c.gpr[4] as u16 != 0),
            },
            Service::TickCount => Call::Ticks,
            Service::ExitToShell => Call::Exit(0),
            Service::NewHandle | Service::NewHandleClear => Call::NewHandle {
                size: c.gpr[3],
                clear: service == Service::NewHandleClear,
            },
            Service::DisposeHandle => Call::DisposeHandle(c.gpr[3]),
            Service::SetHandleSize => Call::SetHandleSize {
                handle: c.gpr[3],
                size: c.gpr[4],
            },
            Service::GetHandleSize => Call::HandleSize(c.gpr[3]),
            Service::GetResource => Call::GetResource {
                kind: c.gpr[3].to_be_bytes(),
                id: c.gpr[4] as i16,
            },
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
                        | Service::Port(_)
                        | Service::PenMode
                        | Service::ExitToShell
                        | Service::SetHandleSize
                        | Service::DisposeHandle
                        | Service::FlushEvents
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
