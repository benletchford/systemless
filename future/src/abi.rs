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
    StackWord(GuestPtr),
    PpcR3,
    M68kHandle,
    M68kA0,
    M68kFileError,
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
            if matches!(service, Service::ExitToShell | Service::DisposeHandle) {
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
            Self::StackLong(p) => memory.write(p, &value.to_be_bytes())?,
            Self::StackWord(p) => memory.write(p, &(value as u16).to_be_bytes())?,
            Self::M68kFileError => match cpu {
                Cpu::M68k(c) => {
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
