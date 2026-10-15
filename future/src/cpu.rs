use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
use m68k::{AddressBus, CpuCore, CpuType, StepResult};
use ppc::{PpcCpu, PpcStepResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    M68k,
    PowerPc,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    CreateResourceFile(GuestPtr),
    Decimal(crate::decimal::Op),
    ReadDateTime(GuestPtr),
    Gestalt(u32),
    StillDown,
    Random,
    DrawRectangle {
        rectangle: GuestPtr,
        op: crate::quickdraw::RectangleOp,
    },
    PenLine {
        h: i16,
        v: i16,
        relative: bool,
    },
    CopyBits(crate::quickdraw::CopyBits),
    CopyMask(crate::quickdraw::CopyMask),
    DrawPicture {
        handle: u32,
        destination: GuestPtr,
    },
    EmptyRect(GuestPtr),
    PointInRect {
        point: [i16; 2],
        rectangle: GuestPtr,
    },
    FindWindow {
        point: [i16; 2],
        destination: GuestPtr,
    },
    GetMouse(GuestPtr),
    GetWMgrPort(GuestPtr),
    StandardPutFile(crate::standard_file::PutFile),
    SystemTask,
    Rectangle(crate::rectangles::Op),
    ToolboxEvent {
        mask: u16,
        destination: GuestPtr,
        remove: bool,
    },
    SysEnvirons {
        version: i16,
        record: GuestPtr,
    },
    MaxApplZone,
    GetApplLimit,
    MoveBytes {
        source: GuestPtr,
        destination: GuestPtr,
        count: i32,
    },
    Dialog(crate::dialogs::Op),
    Cursor(crate::cursor::Op),
    Menu(crate::menus::Op),
    MenuSelect([i16; 2]),
    HiliteMenu(i16),
    MenuChoice,
    Word {
        value: u32,
        high: bool,
    },
    TextEdit(crate::textedit::Op),
    TextDraw(crate::text::Input),
    TextWidth(crate::text::Input),
    MoveTo {
        h: i16,
        v: i16,
    },
    InitFonts,
    FontInfo(GuestPtr),
    TextState {
        field: crate::text::Field,
        value: u16,
    },
    NewWindow(crate::windows::NewWindow),
    WindowInvalidation {
        rectangle: GuestPtr,
        valid: bool,
    },
    WindowReference {
        pointer: u32,
        value: Option<u32>,
    },
    WindowChange {
        pointer: u32,
        change: crate::windows::Change,
    },
    TrackControl(crate::controls::TrackControl),
    NewControl(crate::controls::NewControl),
    Control(crate::controls::Op),
    ResourceWindow {
        id: i16,
        storage: GuestPtr,
        behind: u32,
    },
    Window {
        op: crate::windows::Op,
        pointer: u32,
    },
    PortColor {
        background: bool,
        color: u32,
    },
    RoundRect {
        pointer: GuestPtr,
        width: i16,
        height: i16,
        frame: bool,
    },
    FragmentQuery(crate::cfm::Query),
    LoadFragment(crate::cfm::LoadRequest),
    CloseFragment(GuestPtr),
    InitGraf {
        slot: GuestPtr,
        anchor: Option<GuestPtr>,
    },
    Port {
        op: crate::quickdraw::PortOp,
        pointer: GuestPtr,
    },
    Polygon(crate::polygons::Op),
    PenVisibility(i16),
    PenNormal,
    PenMode(i16),
    SetOrigin {
        h: i16,
        v: i16,
    },
    MovePortTo {
        left: i16,
        top: i16,
    },
    PortSize {
        width: i16,
        height: i16,
    },
    PenSize {
        width: i16,
        height: i16,
    },
    CombineRegions {
        op: crate::region_geometry::Boolean,
        a: u32,
        b: u32,
        destination: u32,
    },
    WaitEvent {
        mask: u16,
        destination: GuestPtr,
        sleep: u32,
        region: GuestPtr,
    },
    PostEvent {
        what: u16,
        message: u32,
    },
    OSEvent {
        mask: u16,
        destination: GuestPtr,
        remove: bool,
    },
    FlushEvents {
        which: u16,
        stop: u16,
    },
    SetEventMask(u16),
    GetEventQueue,

    CExit(u32),
    BreakPoint(GuestPtr),
    SetJump(GuestPtr),
    LongJump {
        buffer: GuestPtr,
        value: u32,
    },
    Add(u32),
    Ticks,
    TrapAddress {
        number: u16,
        toolbox: bool,
    },
    SetToolboxTrap {
        number: u16,
        target: u32,
    },
    NewPointer {
        size: u32,
        clear: bool,
    },
    PointerSize(u32),
    SetPointerSize {
        pointer: u32,
        size: u32,
    },
    DisposePointer(u32),
    NewHandle {
        size: u32,
        clear: bool,
    },
    DisposeHandle(u32),
    SetHandleLock {
        handle: u32,
        locked: bool,
    },
    ReleaseResource(u32),
    DetachResource(u32),
    ResourceInfo(crate::process::ResourceInfo),
    Region {
        op: crate::regions::Op,
        a: u32,
        b: u32,
    },
    HandleSize(u32),
    SetHandleSize {
        handle: u32,
        size: u32,
    },
    GetResource {
        kind: [u8; 4],
        id: i16,
    },
    SoundVolume(i16),
    GetSoundVolume(GuestPtr),
    DeviceControl(GuestPtr),
    StripAddress(u32),
    MemError,
    ResError,
    CurResFile,
    GetNamedResource {
        kind: [u8; 4],
        name: GuestPtr,
        current_only: bool,
    },
    FileIo {
        pointer: GuestPtr,
        op: crate::files::Op,
    },
    FcbInfo(GuestPtr),
    WorkingDirectory {
        pointer: GuestPtr,
        op: crate::files::WorkingOp,
    },
    CatalogInfo(GuestPtr),
    FileInfo {
        pointer: GuestPtr,
        hierarchical: bool,
    },
    CreateDirectory(GuestPtr),
    Draw(GuestPtr),
    Callback(GuestPtr),
    Exit(u32),
}
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Service(Call),
    Macintosh(Call, crate::abi::Reply),
    Return,
}
// Private fixture import gates, not Macintosh trap numbers or CFM descriptors.
pub const IMPORT_BASE: u32 = 0x8000;
pub const RETURN_GATE: u32 = 0x8100;
pub enum Cpu {
    M68k(Box<CpuCore>),
    PowerPc(Box<PpcCpu>),
}
impl Cpu {
    pub fn new(arch: Architecture, pc: u32, sp: u32) -> Self {
        match arch {
            Architecture::M68k => {
                let mut c = Box::new(CpuCore::new());
                c.set_cpu_type(CpuType::M68020);
                c.set_sr(0);
                c.pc = pc;
                c.dar[15] = sp;
                Self::M68k(c)
            }
            Architecture::PowerPc => {
                let mut c = Box::new(PpcCpu::new());
                // Classic Macintosh ABI structures are packed to two bytes.
                // Emulate data alignment fixups through the checked memory bus.
                c.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
                c.pc = pc;
                c.gpr[1] = sp;
                c.lr = RETURN_GATE;
                Self::PowerPc(c)
            }
        }
    }
    pub fn architecture(&self) -> Architecture {
        match self {
            Self::M68k(_) => Architecture::M68k,
            Self::PowerPc(_) => Architecture::PowerPc,
        }
    }
    pub fn pc(&self) -> u32 {
        match self {
            Self::M68k(c) => c.pc,
            Self::PowerPc(c) => c.pc,
        }
    }
    pub fn result(&self) -> u32 {
        match self {
            Self::M68k(c) => c.dar[0],
            Self::PowerPc(c) => c.gpr[3],
        }
    }
    pub fn set_result(&mut self, v: u32) {
        match self {
            Self::M68k(c) => c.dar[0] = v,
            Self::PowerPc(c) => c.gpr[3] = v,
        }
    }
    pub fn step(&mut self, memory: &mut Memory) -> Result<Outcome> {
        self.step_with_abi(memory, &crate::abi::Abi::fixtures())
    }
    pub fn step_with_abi(&mut self, memory: &mut Memory, abi: &crate::abi::Abi) -> Result<Outcome> {
        if self.pc() == RETURN_GATE {
            return Ok(Outcome::Return);
        }
        match self {
            Self::M68k(c) => {
                memory.fetch(GuestPtr(c.pc), 2, 2)?;
                if !abi.fixture {
                    if let Some(mut opcode) = crate::traps::opcode(c.pc) {
                        // A JSR to a trap-table routine has a return address above
                        // the ordinary trap arguments. Remove it before decoding.
                        let ret = memory.u32(GuestPtr(c.dar[15]))?;
                        c.dar[15] = c.dar[15]
                            .checked_add(4)
                            .ok_or_else(|| Error::Cpu("trap entry stack overflow".into()))?;
                        // OS patch chains carry the original trap word in D1.
                        let original = c.dar[1] as u16;
                        if opcode & 0x800 == 0 && original & 0xf0ff == opcode & 0xf0ff {
                            opcode = original;
                        }
                        let (call, reply) = crate::abi::Abi::trap(opcode, c, memory)?;
                        c.pc = ret;
                        return Ok(Outcome::Macintosh(call, reply));
                    }
                }
                let mut bus = Bus {
                    memory,
                    fault: None,
                };
                let outcome = c.step(&mut bus);
                if let Some(fault) = bus.fault {
                    return Err(fault);
                }
                match outcome {
                    StepResult::Ok { .. } => Ok(Outcome::Continue),
                    StepResult::AlineTrap { opcode } => {
                        if !abi.fixture {
                            let mut opcode = opcode;
                            if opcode & 0x800 != 0 {
                                let target = crate::traps::lookup(bus.memory, opcode, true)?;
                                let auto_pop = opcode & 0x400 != 0;
                                opcode &= !0x400;
                                if target != crate::traps::address(opcode, true) {
                                    // A Toolbox patch receives the return PC above its
                                    // Pascal arguments. Auto-pop glue already has it.
                                    bus.memory.fetch(GuestPtr(target), 2, 2)?;
                                    if !auto_pop {
                                        let sp = c.dar[15].checked_sub(4).ok_or_else(|| {
                                            Error::Cpu("trap patch stack underflow".into())
                                        })?;
                                        bus.memory.write(GuestPtr(sp), &c.pc.to_be_bytes())?;
                                        c.dar[15] = sp;
                                    }
                                    c.pc = target;
                                    return Ok(Outcome::Continue);
                                }
                                if auto_pop {
                                    let ret = bus.memory.u32(GuestPtr(c.dar[15]))?;
                                    c.dar[15] = c.dar[15].checked_add(4).ok_or_else(|| {
                                        Error::Cpu("auto-pop stack overflow".into())
                                    })?;
                                    c.pc = ret;
                                }
                            }
                            let (call, reply) = crate::abi::Abi::trap(opcode, c, bus.memory)?;
                            return Ok(Outcome::Macintosh(call, reply));
                        }
                        let argument = bus.memory.u32(GuestPtr(c.dar[15]))?;
                        let call = decode(u32::from(opcode & 0xfff), argument)?;
                        c.dar[15] = c.dar[15]
                            .checked_add(4)
                            .ok_or_else(|| Error::Cpu("stack overflow".into()))?;
                        Ok(Outcome::Service(call))
                    }
                    other => Err(Error::Cpu(format!("68K {other:?}"))),
                }
            }
            Self::PowerPc(c) => {
                if let Some(service) = abi.imports.get(&c.pc) {
                    let (call, reply) = crate::abi::Abi::import(*service, c, memory)?;
                    c.pc = c.lr;
                    return Ok(Outcome::Macintosh(call, reply));
                }
                if abi.fixture && (IMPORT_BASE..IMPORT_BASE + 16).contains(&c.pc) && c.pc % 4 == 0 {
                    let call = decode((c.pc - IMPORT_BASE) / 4, c.gpr[3])?;
                    c.pc = c.lr;
                    return Ok(Outcome::Service(call));
                }
                let word =
                    u32::from_be_bytes(memory.fetch(GuestPtr(c.pc), 4, 4)?.try_into().unwrap());
                let mut bus = Bus {
                    memory,
                    fault: None,
                };
                let outcome = c.step(&mut bus, word);
                if let Some(fault) = bus.fault {
                    return Err(fault);
                }
                match outcome {
                    PpcStepResult::Stepped => Ok(Outcome::Continue),
                    other => Err(Error::Cpu(format!("PPC {other:?}"))),
                }
            }
        }
    }
}
fn decode(id: u32, arg: u32) -> Result<Call> {
    match id {
        0 => Ok(Call::Add(arg)),
        1 => Ok(Call::Draw(GuestPtr(arg))),
        2 => Ok(Call::Callback(GuestPtr(arg))),
        3 => Ok(Call::Exit(arg)),
        _ => Err(Error::Service(id)),
    }
}

// The engines borrow the same authoritative mapping. Retain the first fault;
// suppress further transactions while a 68K engine unwinds an exception.
struct Bus<'a> {
    memory: &'a mut Memory,
    fault: Option<Error>,
}
impl Bus<'_> {
    fn read<const N: usize>(&mut self, a: u32) -> Option<[u8; N]> {
        if self.fault.is_some() {
            return None;
        }
        match self.memory.read(GuestPtr(a), N) {
            Ok(v) => Some(v.try_into().unwrap()),
            Err(e) => {
                self.fault = Some(e);
                None
            }
        }
    }
    fn fetch<const N: usize>(&mut self, a: u32) -> Option<[u8; N]> {
        if self.fault.is_some() {
            return None;
        }
        match self.memory.fetch(GuestPtr(a), N, 2) {
            Ok(v) => Some(v.try_into().unwrap()),
            Err(e) => {
                self.fault = Some(e);
                None
            }
        }
    }
    fn write(&mut self, a: u32, b: &[u8]) -> Option<()> {
        if self.fault.is_some() {
            return None;
        }
        match self.memory.write(GuestPtr(a), b) {
            Ok(()) => Some(()),
            Err(e) => {
                self.fault = Some(e);
                None
            }
        }
    }
}
impl AddressBus for Bus<'_> {
    fn read_immediate_word(&mut self, a: u32) -> u16 {
        u16::from_be_bytes(self.fetch(a).unwrap_or([0; 2]))
    }
    fn read_immediate_long(&mut self, a: u32) -> u32 {
        u32::from_be_bytes(self.fetch(a).unwrap_or([0; 4]))
    }
    fn read_byte(&mut self, a: u32) -> u8 {
        self.read::<1>(a).unwrap_or([0])[0]
    }
    fn read_word(&mut self, a: u32) -> u16 {
        u16::from_be_bytes(self.read(a).unwrap_or([0; 2]))
    }
    fn read_long(&mut self, a: u32) -> u32 {
        u32::from_be_bytes(self.read(a).unwrap_or([0; 4]))
    }
    fn write_byte(&mut self, a: u32, v: u8) {
        self.write(a, &[v]);
    }
    fn write_word(&mut self, a: u32, v: u16) {
        self.write(a, &v.to_be_bytes());
    }
    fn write_long(&mut self, a: u32, v: u32) {
        self.write(a, &v.to_be_bytes());
    }
}
impl ppc::PpcMemory for Bus<'_> {
    fn read_u8(&mut self, a: u32) -> Option<u8> {
        Some(self.read::<1>(a)?[0])
    }
    fn read_u16_be(&mut self, a: u32) -> Option<u16> {
        Some(u16::from_be_bytes(self.read(a)?))
    }
    fn read_u32_be(&mut self, a: u32) -> Option<u32> {
        Some(u32::from_be_bytes(self.read(a)?))
    }
    fn read_u64_be(&mut self, a: u32) -> Option<u64> {
        Some(u64::from_be_bytes(self.read(a)?))
    }
    fn write_u8(&mut self, a: u32, v: u8) -> Option<()> {
        self.write(a, &[v])
    }
    fn write_u16_be(&mut self, a: u32, v: u16) -> Option<()> {
        self.write(a, &v.to_be_bytes())
    }
    fn write_u32_be(&mut self, a: u32, v: u32) -> Option<()> {
        self.write(a, &v.to_be_bytes())
    }
    fn write_u64_be(&mut self, a: u32, v: u64) -> Option<()> {
        self.write(a, &v.to_be_bytes())
    }
}
