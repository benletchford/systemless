use crate::{
    cpu::{Architecture, Call, Cpu, Outcome, RETURN_GATE},
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Budget,
    Waiting { ticks: u32 },
    Exit(u32),
    Returned(u32),
}
#[derive(Default, Debug)]
pub struct Services {
    pub accumulator: u32,
    pub calls: Vec<Call>,
}
// The whole CPU is suspended, retaining even engine-private FPU/control state.
struct Continuation {
    caller: Cpu,
    resume: Resume,
}
enum Resume {
    Fixture,
    Loaded {
        connection: u32,
        request: crate::cfm::LoadRequest,
        reply: crate::abi::Reply,
    },
    Closed {
        connection: u32,
        destination: GuestPtr,
        reply: crate::abi::Reply,
    },
    Shutdown {
        connection: u32,
        stop: Stop,
    },
}
struct EventWait {
    mask: u16,
    destination: GuestPtr,
    reply: crate::abi::Reply,
    remaining: u32,
}
pub struct Runtime {
    pub memory: Memory,
    pub services: Services,
    pub graphics: Option<Graphics>,
    pub cpu: Cpu,
    pub instructions: u64,
    pub abi: crate::abi::Abi,
    pub process: crate::process::Process,
    continuations: Vec<Continuation>,
    event_wait: Option<EventWait>,
    terminal: Option<Stop>,
    pub(crate) lifecycle: Option<crate::lifecycle::Lifecycle>,
    faulted: bool,
}
impl Runtime {
    pub fn new(memory: Memory, cpu: Cpu, graphics: Option<Graphics>) -> Self {
        Self {
            memory,
            cpu,
            graphics,
            services: Services::default(),
            instructions: 0,
            abi: crate::abi::Abi::fixtures(),
            process: crate::process::Process::default(),
            continuations: Vec::new(),
            event_wait: None,
            terminal: None,
            lifecycle: None,
            faulted: false,
        }
    }
    /// Refresh the host display from canonical screen bytes, even after exit.
    /// Offscreen ports and their last drawing texture do not select the display.
    pub fn refresh_screen(&mut self) -> Result<()> {
        let gpu = self
            .graphics
            .as_ref()
            .ok_or_else(|| Error::Graphics("screen refresh requires GPU".into()))?;
        self.process.cursor.observe(self.process.input);
        crate::quickdraw::screen_bitmap().present_draw(
            gpu,
            &self.memory,
            self.process.cursor.draw(self.process.input),
        )
    }
    pub fn advance(&mut self, budget: u64) -> Result<Stop> {
        if self.faulted {
            return Err(Error::Cpu("runtime stopped after earlier fault".into()));
        }
        if let Some(stop) = self.terminal {
            return Ok(stop);
        }
        let result = self.advance_inner(budget);
        if result.is_err() {
            self.faulted = true;
        }
        result
    }
    /// Host-controlled deterministic time, including periods with no guest CPU
    /// execution. A host can inject events before advancing this clock.
    pub fn elapse_ticks(&mut self, ticks: u32) -> Result<()> {
        if self.faulted || self.terminal.is_some() {
            return Err(Error::Cpu("cannot advance a stopped process clock".into()));
        }
        let now = self.memory.u32(GuestPtr(0x16a))?;
        self.memory
            .write(GuestPtr(0x16a), &now.wrapping_add(ticks).to_be_bytes())?;
        if let Some(wait) = &mut self.event_wait {
            wait.remaining = wait.remaining.saturating_sub(ticks);
        }
        Ok(())
    }
    fn advance_inner(&mut self, budget: u64) -> Result<Stop> {
        if budget == 0 {
            return Ok(Stop::Budget);
        }
        if let Some(wait) = self.event_wait.take() {
            if let Some(stop) = self.poll_event_wait(wait)? {
                return Ok(stop);
            }
        }

        for _ in 0..budget {
            self.instructions = self
                .instructions
                .checked_add(1)
                .ok_or_else(|| Error::Cpu("instruction clock overflow".into()))?;
            if !self.abi.fixture && self.instructions.is_multiple_of(1000) {
                let ticks = self.memory.u32(GuestPtr(0x16a))?.wrapping_add(1);
                self.memory.write(GuestPtr(0x16a), &ticks.to_be_bytes())?;
            }
            match self.cpu.step_with_abi(&mut self.memory, &self.abi)? {
                Outcome::Continue => {}
                Outcome::Return => {
                    let value = self.cpu.result();
                    if let Some(frame) = self.continuations.pop() {
                        if let Some(stop) = self.resume(frame, value)? {
                            return Ok(stop);
                        }
                    } else {
                        let stop = if let Some(lifecycle) = &mut self.lifecycle {
                            lifecycle.returned(&mut self.cpu)?
                        } else {
                            Some(Stop::Returned(value))
                        };
                        if let Some(stop) = stop {
                            if let Some(stop) = self.finish_process(stop)? {
                                return Ok(stop);
                            }
                        }
                    }
                }
                Outcome::Macintosh(call, reply) => {
                    self.services.calls.push(call);
                    match call {
                        Call::RoundRect {
                            pointer,
                            width,
                            height,
                            frame,
                        } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            let r = crate::quickdraw::read_rect(&self.memory, pointer)?;
                            let gpu = self.graphics.as_ref().ok_or_else(|| {
                                Error::Graphics("QuickDraw painting requires a GPU".into())
                            })?;
                            ports.round_rect(gpu, &mut self.memory, r, width, height, frame)?;
                        }
                        Call::PortColor { background, color } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            ports.set_color(&mut self.memory, background, color)?;
                        }
                        Call::WindowInvalidation { rectangle, valid } => {
                            let windows = std::mem::take(&mut self.process.windows);
                            let result = windows.invalidate(
                                &mut self.memory,
                                &mut self.process,
                                rectangle,
                                valid,
                            );
                            self.process.windows = windows;
                            result?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::WindowReference { pointer, value } => {
                            let result =
                                self.process
                                    .windows
                                    .reference(&mut self.memory, pointer, value)?;
                            reply.apply(&mut self.cpu, &mut self.memory, result, 0)?;
                        }
                        Call::NewControl(_) | Call::Control(_) => {
                            let mut controls = std::mem::take(&mut self.process.controls);
                            let result = match call {
                                Call::NewControl(request) => controls.create(
                                    &mut self.memory,
                                    &mut self.process,
                                    self.graphics.as_ref(),
                                    request,
                                ),
                                Call::Control(op) => controls.call(
                                    &mut self.memory,
                                    &mut self.process,
                                    self.graphics.as_ref(),
                                    op,
                                ),
                                _ => unreachable!(),
                            };
                            self.process.controls = controls;
                            reply.apply(&mut self.cpu, &mut self.memory, result?, 0)?;
                        }
                        Call::NewWindow(_)
                        | Call::WindowChange { .. }
                        | Call::ResourceWindow { .. }
                        | Call::Window { .. } => {
                            let gpu = self.graphics.as_ref().ok_or_else(|| {
                                Error::Graphics("Window Manager requires a GPU".into())
                            })?;
                            let mut windows = std::mem::take(&mut self.process.windows);
                            let result = match call {
                                Call::WindowChange { pointer, change } => windows
                                    .change(
                                        &mut self.memory,
                                        &mut self.process,
                                        gpu,
                                        pointer,
                                        change,
                                    )
                                    .map(|()| 0),
                                Call::ResourceWindow {
                                    id,
                                    storage,
                                    behind,
                                } => windows.from_resource(
                                    &mut self.memory,
                                    &mut self.process,
                                    gpu,
                                    id,
                                    storage,
                                    behind,
                                ),
                                Call::NewWindow(request) => windows.create(
                                    &mut self.memory,
                                    &mut self.process,
                                    gpu,
                                    request,
                                ),
                                Call::Window { op, pointer } => windows.call(
                                    &mut self.memory,
                                    &mut self.process,
                                    gpu,
                                    op,
                                    pointer,
                                ),
                                _ => unreachable!(),
                            };
                            self.process.windows = windows;
                            let value = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::TextState { field, value } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            crate::text::set(&mut self.memory, ports, field, value)?;
                        }
                        Call::FontInfo(destination) => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            crate::text::font_info(
                                &mut self.memory,
                                &mut self.process,
                                ports,
                                destination,
                            )?;
                        }
                        Call::InitFonts => {
                            crate::font::initialize(&mut self.memory, &mut self.process)?
                        }
                        Call::TextDraw(input) | Call::TextWidth(input) => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            if matches!(call, Call::TextWidth(_)) {
                                let value = crate::text::width(
                                    &mut self.memory,
                                    &mut self.process,
                                    ports,
                                    input,
                                )?;
                                reply.apply(
                                    &mut self.cpu,
                                    &mut self.memory,
                                    value as i32 as u32,
                                    0,
                                )?;
                            } else {
                                let gpu = self.graphics.as_ref().ok_or_else(|| {
                                    Error::Graphics("text drawing requires a GPU".into())
                                })?;
                                crate::text::draw(
                                    &mut self.memory,
                                    &mut self.process,
                                    ports,
                                    gpu,
                                    input,
                                )?;
                            }
                        }
                        Call::PenLine { h, v, relative } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            ports.line(&mut self.memory, self.graphics.as_ref(), h, v, relative)?;
                        }
                        Call::MoveTo { h, v } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            crate::text::move_to(&mut self.memory, ports, h, v)?;
                        }
                        Call::Rectangle(op) => {
                            crate::rectangles::call(&mut self.memory, op)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ToolboxEvent {
                            mask,
                            destination,
                            remove,
                        } => {
                            let ticks = self.tick_count()?;
                            let found =
                                self.receive_toolbox_event(mask, destination, remove, ticks)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(found), 0)?;
                        }
                        Call::PointInRect { point, rectangle } => {
                            let inside =
                                crate::rectangles::contains(&self.memory, point, rectangle)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(inside), 0)?;
                        }
                        Call::GetMouse(destination) => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("GetMouse requires InitGraf".into())
                            })?;
                            let global: [u8; 4] = self
                                .memory
                                .read(crate::events::MOUSE, 4)?
                                .try_into()
                                .unwrap();
                            let local = ports.point_coordinates(&self.memory, global, false)?;
                            self.memory.write(destination, &local)?;
                            self.process.input.vertical =
                                i16::from_be_bytes(global[..2].try_into().unwrap());
                            self.process.input.horizontal =
                                i16::from_be_bytes(global[2..].try_into().unwrap());
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::SystemTask => {
                            // There are no registered desk accessories or periodic device
                            // drivers in this runtime. SystemTask has no work to dispatch;
                            // ordinary instruction/host tick progression remains authoritative.
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::SysEnvirons { version, record } => {
                            let result =
                                crate::environment::query(&mut self.memory, version, record)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                result as i32 as u32,
                                0,
                            )?;
                        }
                        Call::MaxApplZone => {
                            self.process.maximize_heap(&mut self.memory)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::GetApplLimit => {
                            let value = self.process.application_limit(&self.memory)?;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::MoveBytes {
                            source,
                            destination,
                            count,
                        } => {
                            self.memory.move_bytes(source, destination, count)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::Dialog(op) => {
                            let value = if let crate::dialogs::Op::Slot(index) = op {
                                crate::dialogs::slot(&self.memory, index)?
                            } else {
                                let mut manager = std::mem::take(&mut self.process.dialogs);
                                let result = manager.call(&mut self.memory, &mut self.process, op);
                                self.process.dialogs = manager;
                                result?;
                                0
                            };
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::Cursor(op) => {
                            crate::cursor::call(&self.memory, &mut self.process, op)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::Menu(op) => {
                            let mut manager = std::mem::take(&mut self.process.menus);
                            let result = manager.call(
                                &mut self.memory,
                                &mut self.process,
                                self.graphics.as_ref(),
                                op,
                            );
                            self.process.menus = manager;
                            let value = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::TextEdit(op) => {
                            let ticks = self.tick_count()?;
                            let mut manager = std::mem::take(&mut self.process.textedit);
                            let result = match op {
                                crate::textedit::Op::TextBox(request) => {
                                    match self.graphics.as_ref() {
                                        Some(gpu) => manager
                                            .text_box(
                                                &mut self.memory,
                                                &mut self.process,
                                                gpu,
                                                request,
                                                ticks,
                                            )
                                            .map(|()| 0),
                                        None => Err(Error::Graphics(
                                            "text box drawing requires a GPU".into(),
                                        )),
                                    }
                                }
                                crate::textedit::Op::Key { handle, byte } => {
                                    match self.graphics.as_ref() {
                                        Some(gpu) => manager
                                            .key(
                                                &mut self.memory,
                                                &mut self.process,
                                                gpu,
                                                handle,
                                                byte,
                                                ticks,
                                            )
                                            .map(|()| 0),
                                        None => Err(Error::Graphics(
                                            "TextEdit key editing requires a GPU".into(),
                                        )),
                                    }
                                }
                                crate::textedit::Op::Idle(handle) => manager
                                    .idle(
                                        &mut self.memory,
                                        &mut self.process,
                                        self.graphics.as_ref(),
                                        handle,
                                        ticks,
                                    )
                                    .map(|()| 0),
                                crate::textedit::Op::Select { start, end, handle } => manager
                                    .select(
                                        &mut self.memory,
                                        &mut self.process,
                                        self.graphics.as_ref(),
                                        handle,
                                        (start, end),
                                        ticks,
                                    )
                                    .map(|()| 0),
                                crate::textedit::Op::Activate { handle, active } => {
                                    match self.graphics.as_ref() {
                                        Some(gpu) => manager
                                            .activate(
                                                &mut self.memory,
                                                &mut self.process,
                                                gpu,
                                                handle,
                                                active,
                                                ticks,
                                            )
                                            .map(|()| 0),
                                        None => Err(Error::Graphics(
                                            "TextEdit activation requires a GPU".into(),
                                        )),
                                    }
                                }
                                crate::textedit::Op::Delete(handle) => {
                                    match self.graphics.as_ref() {
                                        Some(gpu) => manager
                                            .delete(
                                                &mut self.memory,
                                                &mut self.process,
                                                gpu,
                                                handle,
                                                ticks,
                                            )
                                            .map(|()| 0),
                                        None => Err(Error::Graphics(
                                            "TextEdit editing requires a GPU".into(),
                                        )),
                                    }
                                }
                                crate::textedit::Op::Update { rect, handle } => {
                                    match self.graphics.as_ref() {
                                        Some(gpu) => manager
                                            .update(
                                                &mut self.memory,
                                                &mut self.process,
                                                gpu,
                                                rect,
                                                handle,
                                            )
                                            .map(|()| 0),
                                        None => Err(Error::Graphics(
                                            "TextEdit redraw requires a GPU".into(),
                                        )),
                                    }
                                }
                                _ => manager.call(&mut self.memory, &mut self.process, op, ticks),
                            };
                            self.process.textedit = manager;
                            let value = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::Random => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            let value = ports.random(&mut self.memory)?;
                            reply.apply(&mut self.cpu, &mut self.memory, value as i32 as u32, 0)?;
                        }
                        Call::LoadFragment(request) => {
                            self.validate_fragment_call()?;
                            match self.process.fragments.load_memory(
                                &mut self.memory,
                                &mut self.abi,
                                request,
                            )? {
                                crate::cfm::LoadAction::Complete(error) => reply.apply(
                                    &mut self.cpu,
                                    &mut self.memory,
                                    error as i32 as u32,
                                    error,
                                )?,
                                crate::cfm::LoadAction::Initialize {
                                    connection,
                                    routine,
                                    block,
                                } => {
                                    self.enter_fragment(
                                        routine,
                                        Some(block),
                                        Resume::Loaded {
                                            connection,
                                            request,
                                            reply,
                                        },
                                    )?;
                                }
                            }
                        }
                        Call::CloseFragment(destination) => {
                            self.validate_fragment_call()?;
                            match self.process.fragments.close(
                                &mut self.memory,
                                &mut self.abi,
                                destination,
                            )? {
                                crate::cfm::CloseAction::Complete(error) => reply.apply(
                                    &mut self.cpu,
                                    &mut self.memory,
                                    error as i32 as u32,
                                    error,
                                )?,
                                crate::cfm::CloseAction::Terminate {
                                    connection,
                                    routine,
                                } => {
                                    self.enter_fragment(
                                        routine,
                                        None,
                                        Resume::Closed {
                                            connection,
                                            destination,
                                            reply,
                                        },
                                    )?;
                                }
                            }
                        }
                        Call::FragmentQuery(query) => {
                            let error = self.process.fragments.query(&mut self.memory, query)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::CombineRegions {
                            op,
                            a,
                            b,
                            destination,
                        } => {
                            crate::regions::combine(
                                &mut self.memory,
                                &mut self.process,
                                op,
                                a,
                                b,
                                destination,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::Region { op, a, b } => {
                            let value = crate::regions::call(
                                &mut self.memory,
                                &mut self.process,
                                op,
                                a,
                                b,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::InitGraf { slot, anchor } => {
                            self.process.ports = Some(crate::quickdraw::initialize(
                                &mut self.memory,
                                slot,
                                anchor,
                            )?)
                        }
                        Call::Port { op, pointer } => {
                            use crate::quickdraw::{read_rect, PortOp};
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            match op {
                                PortOp::LocalToGlobal | PortOp::GlobalToLocal => {
                                    ports.convert_point(
                                        &mut self.memory,
                                        pointer,
                                        op == PortOp::LocalToGlobal,
                                    )?;
                                }
                                PortOp::Open => {
                                    ports.open(&mut self.memory, &mut self.process, pointer)?
                                }
                                PortOp::Init => {
                                    ports.init(&mut self.memory, &mut self.process, pointer)?;
                                }
                                PortOp::Close => {
                                    ports.close(&mut self.memory, &mut self.process, pointer)?
                                }
                                PortOp::Set => ports.set(&mut self.memory, pointer)?,
                                PortOp::Get => {
                                    let p = self.memory.u32(ports.current_slot)?;
                                    self.memory.write(pointer, &p.to_be_bytes())?;
                                }
                                PortOp::SetBits => ports.set_bits(&mut self.memory, pointer)?,
                                PortOp::ClipRect => {
                                    let r = read_rect(&self.memory, pointer)?;
                                    ports.clip_rect(&mut self.memory, r)?;
                                }
                                PortOp::PenPat => {
                                    let pattern: [u8; 8] =
                                        self.memory.read(pointer, 8)?.try_into().unwrap();
                                    let p = ports.current(&self.memory)?;
                                    self.memory.write(GuestPtr(p.0 + 58), &pattern)?;
                                }
                                PortOp::PaintRect => {
                                    let r = read_rect(&self.memory, pointer)?;
                                    let gpu = self.graphics.as_ref().ok_or_else(|| {
                                        Error::Graphics("QuickDraw painting requires a GPU".into())
                                    })?;
                                    ports.paint_rect(gpu, &mut self.memory, r)?;
                                }
                            }
                        }
                        Call::DrawRectangle { rectangle, op } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            let gpu = self.graphics.as_ref().ok_or_else(|| {
                                Error::Graphics("rectangle drawing requires a GPU".into())
                            })?;
                            ports.draw_rectangle(&mut self.memory, gpu, rectangle, op)?;
                        }
                        Call::CopyBits(request) => {
                            // Legacy comparison contract treats NIL bitmap parameters as inert.
                            if request.source.0 != 0 && request.destination.0 != 0 {
                                let ports = self.process.ports.ok_or_else(|| {
                                    Error::Graphics("QuickDraw has not been initialized".into())
                                })?;
                                let gpu = self.graphics.as_ref().ok_or_else(|| {
                                    Error::Graphics("CopyBits requires a GPU".into())
                                })?;
                                ports.copy_bits(&mut self.memory, gpu, request)?;
                            }
                        }
                        Call::DrawPicture {
                            handle,
                            destination,
                        } => {
                            if handle != 0 {
                                let ports = self.process.ports.ok_or_else(|| {
                                    Error::Graphics("QuickDraw has not been initialized".into())
                                })?;
                                let gpu = self.graphics.as_ref().ok_or_else(|| {
                                    Error::Graphics("DrawPicture requires a GPU".into())
                                })?;
                                crate::picture::draw(
                                    &mut self.memory,
                                    &mut self.process,
                                    ports,
                                    gpu,
                                    handle,
                                    destination,
                                )?;
                            }
                        }
                        Call::MovePortTo { left, top } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            ports.move_port_to(&mut self.memory, left, top)?;
                        }
                        Call::PortSize { width, height } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            ports.port_size(&mut self.memory, width, height)?;
                        }
                        Call::PenSize { width, height } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            let p = ports.current(&self.memory)?;
                            let mut bytes = [0; 4];
                            bytes[..2].copy_from_slice(&height.to_be_bytes());
                            bytes[2..].copy_from_slice(&width.to_be_bytes());
                            self.memory.write(GuestPtr(p.0 + 52), &bytes)?;
                        }
                        Call::PenMode(mode) => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            let p = ports.current(&self.memory)?;
                            self.memory.write(GuestPtr(p.0 + 56), &mode.to_be_bytes())?;
                        }
                        Call::WaitEvent {
                            mask,
                            destination,
                            sleep,
                            region,
                        } => {
                            if region.0 != 0 {
                                return Err(Error::Cpu(
                                    "WaitNextEvent mouse regions are not implemented".into(),
                                ));
                            }
                            self.memory.validate_write(destination, 16)?;
                            let wait = EventWait {
                                mask,
                                destination,
                                reply,
                                remaining: sleep,
                            };
                            if let Some(stop) = self.poll_event_wait(wait)? {
                                return Ok(stop);
                            }
                        }
                        Call::PostEvent { what, message } => {
                            let ticks = self.tick_count()?;
                            let result = crate::events::post(
                                &mut self.memory,
                                what,
                                message,
                                ticks,
                                self.process.input,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                result as i32 as u32,
                                0,
                            )?;
                        }
                        Call::OSEvent {
                            mask,
                            destination,
                            remove,
                        } => {
                            let ticks = self.tick_count()?;
                            let found = crate::events::receive(
                                &mut self.memory,
                                mask,
                                destination,
                                remove,
                                ticks,
                                self.process.input,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(found), 0)?;
                        }
                        Call::FlushEvents { which, stop } => {
                            let stopped = crate::events::flush(&mut self.memory, which, stop)?;
                            reply.apply(&mut self.cpu, &mut self.memory, stopped.into(), 0)?;
                        }
                        Call::SetEventMask(mask) => self
                            .memory
                            .write(crate::events::SYSTEM_MASK, &mask.to_be_bytes())?,
                        Call::GetEventQueue => reply.apply(
                            &mut self.cpu,
                            &mut self.memory,
                            crate::events::QUEUE_HEADER.0,
                            0,
                        )?,
                        Call::BreakPoint(pointer) => {
                            return Err(Error::Cpu(format!(
                                "PPC debugger breakpoint requested at {:#x}",
                                pointer.0
                            )));
                        }
                        Call::SetJump(buffer) => {
                            crate::jump::save(&mut self.cpu, &mut self.memory, buffer)?
                        }
                        Call::LongJump { buffer, value } => {
                            crate::jump::restore(&mut self.cpu, &self.memory, buffer, value)?
                        }
                        Call::SetToolboxTrap { number, target } => {
                            crate::traps::set_toolbox(&mut self.memory, number, target)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::TrapAddress { number, toolbox } => {
                            let value = crate::traps::lookup(&self.memory, number, toolbox)?;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::Ticks => {
                            let value = self.tick_count()?;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::Exit(v) | Call::CExit(v) => {
                            if self
                                .continuations
                                .iter()
                                .any(|c| !matches!(c.resume, Resume::Fixture))
                            {
                                return Err(Error::Cpu("process exit during CFM init/term".into()));
                            }
                            if matches!(call, Call::CExit(_)) {
                                crate::stdlib::exit(&mut self.memory, v)?;
                            }
                            let stop = if let Some(lifecycle) = &mut self.lifecycle {
                                lifecycle.exit(&mut self.cpu, v)?
                            } else {
                                Some(Stop::Exit(v))
                            };
                            self.continuations.clear();
                            if let Some(stop) = stop {
                                if let Some(stop) = self.finish_process(stop)? {
                                    return Ok(stop);
                                }
                            }
                        }
                        Call::NewPointer { size, clear } => {
                            let value = self.process.new_pointer(&mut self.memory, size, clear)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::PointerSize(pointer) => {
                            let value = self.process.pointer_size(pointer);
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::SetPointerSize { pointer, size } => {
                            self.process.set_pointer_size(pointer, size);
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::DisposePointer(pointer) => {
                            self.process.dispose_pointer(pointer);
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::NewHandle { size, clear } => {
                            let v = self.process.new_handle(&mut self.memory, size, clear)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                v,
                                self.process.mem_error,
                            )?;
                        }
                        Call::SetHandleLock { handle, locked } => {
                            self.process.set_handle_lock(&self.memory, handle, locked)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::DisposeHandle(h) => {
                            self.process.dispose_handle(&mut self.memory, h)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::SetHandleSize { handle, size } => {
                            self.process
                                .set_handle_size(&mut self.memory, handle, size)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::HandleSize(h) => {
                            let v = self.process.handle_size(&self.memory, h)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                v,
                                self.process.mem_error,
                            )?;
                        }
                        Call::GetNamedResource {
                            kind,
                            name,
                            current_only,
                        } => {
                            let value = self.process.get_named_resource(
                                &mut self.memory,
                                kind,
                                name,
                                current_only,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::DetachResource(handle) => {
                            self.process.detach_resource(&self.memory, handle)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ReleaseResource(handle) => {
                            self.process.release_resource(&mut self.memory, handle)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::GetResource { kind, id } => {
                            let v = self.process.get_resource(&mut self.memory, kind, id)?;
                            reply.apply(&mut self.cpu, &mut self.memory, v, 0)?;
                        }
                        Call::SoundVolume(level) => {
                            crate::sound::set_volume(&mut self.memory, level)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::GetSoundVolume(destination) => {
                            crate::sound::get_volume(&mut self.memory, destination)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::DeviceControl(pointer) => {
                            let error = crate::sound::control(&mut self.memory, pointer)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::StripAddress(address) => {
                            // This runtime starts in 32-bit mode. IM: Memory
                            // Utilities specifies an unchanged address in that mode.
                            reply.apply(&mut self.cpu, &mut self.memory, address, 0)?;
                        }
                        Call::MemError => {
                            let value = self.memory.u16(GuestPtr(0x220))? as i16 as i32 as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::CreateDirectory(pointer) => {
                            let error = self
                                .process
                                .files
                                .create_directory(&mut self.memory, pointer)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::FileInfo {
                            pointer,
                            hierarchical,
                        } => {
                            let error = self.process.files.file_info(
                                &mut self.memory,
                                pointer,
                                hierarchical,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::CatalogInfo(pointer) => {
                            let error =
                                self.process.files.catalog_info(&mut self.memory, pointer)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::FcbInfo(pointer) => {
                            let error = self.process.files.fcb_info(&mut self.memory, pointer)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::CurResFile => {
                            let value = self.process.current_resource_file() as i32 as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::ResError => {
                            let value = self.memory.u16(GuestPtr(0xa60))? as i16 as i32 as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        _ => return Err(Error::Cpu("invalid Macintosh service route".into())),
                    }
                    // Guest globals are the ABI-visible error authorities. Process
                    // fields carry operation outputs only, never readback authority.
                    if matches!(
                        call,
                        Call::MaxApplZone
                            | Call::Dialog(
                                crate::dialogs::Op::Init(_) | crate::dialogs::Op::Param(_)
                            )
                            | Call::Menu(_)
                            | Call::TextEdit(_)
                            | Call::TextDraw(_)
                            | Call::TextWidth(_)
                            | Call::InitFonts
                            | Call::FontInfo(_)
                            | Call::ResourceWindow { .. }
                            | Call::NewControl(_)
                            | Call::Control(_)
                            | Call::WindowInvalidation { .. }
                            | Call::WindowChange { .. }
                            | Call::NewWindow(_)
                            | Call::Window { .. }
                            | Call::NewPointer { .. }
                            | Call::PointerSize(_)
                            | Call::SetPointerSize { .. }
                            | Call::DisposePointer(_)
                            | Call::NewHandle { .. }
                            | Call::Port {
                                op: crate::quickdraw::PortOp::Open
                                    | crate::quickdraw::PortOp::Close
                                    | crate::quickdraw::PortOp::Init,
                                ..
                            }
                            | Call::SetHandleLock { .. }
                            | Call::DisposeHandle(_)
                            | Call::CombineRegions { .. }
                            | Call::Region { .. }
                            | Call::HandleSize(_)
                            | Call::SetHandleSize { .. }
                            | Call::GetResource { .. }
                            | Call::GetNamedResource { .. }
                    ) {
                        self.memory
                            .write(GuestPtr(0x220), &self.process.mem_error.to_be_bytes())?;
                    }
                    if matches!(
                        call,
                        Call::Menu(crate::menus::Op::Init)
                            | Call::FontInfo(_)
                            | Call::GetResource { .. }
                            | Call::GetNamedResource { .. }
                            | Call::DetachResource(_)
                            | Call::ReleaseResource(_)
                    ) {
                        self.memory
                            .write(GuestPtr(0xa60), &self.process.res_error.to_be_bytes())?;
                    }
                }
                Outcome::Service(call) => {
                    self.services.calls.push(call);
                    match call {
                        Call::RoundRect { .. }
                        | Call::PortColor { .. }
                        | Call::NewPointer { .. }
                        | Call::PointerSize(_)
                        | Call::SetPointerSize { .. }
                        | Call::DisposePointer(_)
                        | Call::ResourceWindow { .. }
                        | Call::NewControl(_)
                        | Call::Control(_)
                        | Call::WindowInvalidation { .. }
                        | Call::WindowReference { .. }
                        | Call::WindowChange { .. }
                        | Call::NewWindow(_)
                        | Call::Window { .. }
                        | Call::TextState { .. }
                        | Call::Rectangle(_)
                        | Call::ToolboxEvent { .. }
                        | Call::PointInRect { .. }
                        | Call::GetMouse(_)
                        | Call::SystemTask
                        | Call::SysEnvirons { .. }
                        | Call::MaxApplZone
                        | Call::GetApplLimit
                        | Call::MoveBytes { .. }
                        | Call::Dialog(_)
                        | Call::Cursor(_)
                        | Call::Menu(_)
                        | Call::TextEdit(_)
                        | Call::PenLine { .. }
                        | Call::MoveTo { .. }
                        | Call::TextDraw(_)
                        | Call::TextWidth(_)
                        | Call::InitFonts
                        | Call::FontInfo(_)
                        | Call::Random
                        | Call::LoadFragment(_)
                        | Call::CloseFragment(_)
                        | Call::FragmentQuery(_)
                        | Call::InitGraf { .. }
                        | Call::Port { .. }
                        | Call::DrawRectangle { .. }
                        | Call::CopyBits(_)
                        | Call::DrawPicture { .. }
                        | Call::MovePortTo { .. }
                        | Call::PortSize { .. }
                        | Call::PenSize { .. }
                        | Call::PenMode(_)
                        | Call::WaitEvent { .. }
                        | Call::PostEvent { .. }
                        | Call::OSEvent { .. }
                        | Call::FlushEvents { .. }
                        | Call::SetEventMask(_)
                        | Call::GetEventQueue
                        | Call::CExit(_)
                        | Call::BreakPoint(_)
                        | Call::SetJump(_)
                        | Call::LongJump { .. }
                        | Call::SetToolboxTrap { .. }
                        | Call::TrapAddress { .. }
                        | Call::NewHandle { .. }
                        | Call::SetHandleLock { .. }
                        | Call::DisposeHandle(_)
                        | Call::CombineRegions { .. }
                        | Call::Region { .. }
                        | Call::HandleSize(_)
                        | Call::SetHandleSize { .. }
                        | Call::GetResource { .. }
                        | Call::SoundVolume(_)
                        | Call::GetSoundVolume(_)
                        | Call::DeviceControl(_)
                        | Call::StripAddress(_)
                        | Call::MemError
                        | Call::ResError
                        | Call::DetachResource(_)
                        | Call::ReleaseResource(_)
                        | Call::CurResFile
                        | Call::GetNamedResource { .. }
                        | Call::CreateDirectory(_)
                        | Call::FileInfo { .. }
                        | Call::CatalogInfo(_)
                        | Call::FcbInfo(_) => {
                            return Err(Error::Cpu("Macintosh service on fixture route".into()))
                        }
                        Call::Ticks => {
                            let value = self.tick_count()?;
                            self.cpu.set_result(value);
                        }
                        Call::Add(v) => {
                            self.services.accumulator = self.services.accumulator.wrapping_add(v);
                            self.cpu.set_result(self.services.accumulator);
                        }
                        Call::Draw(ptr) => {
                            // Private fixture command record, all fields are big-endian.
                            let b: [u8; 10] = self.memory.read(ptr, 10)?.try_into().unwrap();
                            let coord = |i| i32::from(i16::from_be_bytes([b[i], b[i + 1]]));
                            let rect = Rect::new(coord(0), coord(2), coord(4), coord(6));
                            let gpu = self
                                .graphics
                                .as_ref()
                                .ok_or_else(|| Error::Graphics("drawing requires a GPU".into()))?;
                            let mode = match b[9] {
                                0 => Transfer::Copy,
                                1 => Transfer::Or,
                                2 => Transfer::Xor,
                                3 => Transfer::Bic,
                                v => return Err(Error::Service(v.into())),
                            };
                            gpu.execute(
                                &mut self.memory,
                                Draw::Fill {
                                    rect,
                                    clip: Rect::new(0, 0, gpu.width as i32, gpu.height as i32),
                                    pattern: [255; 8],
                                    foreground: b[8],
                                    background: 0,
                                    mode,
                                },
                            )?;
                            self.cpu.set_result(0);
                        }
                        Call::Callback(ptr) => self.enter_callback(ptr)?,
                        Call::Exit(v) => {
                            let stop = Stop::Exit(v);
                            self.terminal = Some(stop);
                            return Ok(stop);
                        }
                    }
                }
            }
        }
        Ok(Stop::Budget)
    }
    fn toolbox_available(&mut self, mask: u16) -> Result<bool> {
        if crate::events::available(&self.memory, mask)? {
            return Ok(true);
        }
        if mask & (1 << 8) != 0 && crate::windows::Windows::activation(&self.memory)?.is_some() {
            return Ok(true);
        }
        if mask & (1 << 6) == 0 {
            return Ok(false);
        }
        let windows = std::mem::take(&mut self.process.windows);
        let result = windows.update_window(&self.memory, &mut self.process);
        self.process.windows = windows;
        result.map(|window| window.is_some())
    }
    fn receive_toolbox_event(
        &mut self,
        mask: u16,
        destination: GuestPtr,
        remove: bool,
        ticks: u32,
    ) -> Result<bool> {
        self.memory.validate_write(destination, 16)?;
        if crate::events::available(&self.memory, mask)? {
            return crate::events::receive(
                &mut self.memory,
                mask,
                destination,
                remove,
                ticks,
                self.process.input,
            );
        }
        if mask & (1 << 8) != 0 {
            if let Some((slot, window, active)) = crate::windows::Windows::activation(&self.memory)?
            {
                if remove {
                    self.memory.validate_write(slot, 4)?;
                }
                let mut input = self.process.input;
                input.modifiers = (input.modifiers & !1) | u16::from(active);
                self.memory
                    .write(destination, &crate::events::record(8, window, ticks, input))?;
                if remove {
                    self.memory.write(slot, &0u32.to_be_bytes())?;
                }
                return Ok(true);
            }
        }
        if mask & (1 << 6) != 0 {
            let windows = std::mem::take(&mut self.process.windows);
            let result = windows.update_window(&self.memory, &mut self.process);
            self.process.windows = windows;
            if let Some(window) = result? {
                self.memory.write(
                    destination,
                    &crate::events::record(6, window, ticks, self.process.input),
                )?;
                return Ok(true);
            }
        }
        crate::events::receive(
            &mut self.memory,
            mask,
            destination,
            remove,
            ticks,
            self.process.input,
        )
    }
    fn poll_event_wait(&mut self, wait: EventWait) -> Result<Option<Stop>> {
        let ready = self.toolbox_available(wait.mask)?;
        if !ready && wait.remaining != 0 {
            let ticks = wait.remaining;
            self.event_wait = Some(wait);
            return Ok(Some(Stop::Waiting { ticks }));
        }
        // Validate deferred ABI output before removing an event.
        if let crate::abi::Reply::StackByte(p) = wait.reply {
            self.memory.validate_write(p, 1)?;
        }
        let now = self.tick_count()?;
        let found = self.receive_toolbox_event(wait.mask, wait.destination, true, now)?;
        wait.reply
            .apply(&mut self.cpu, &mut self.memory, u32::from(found), 0)?;
        Ok(None)
    }
    fn validate_fragment_call(&self) -> Result<()> {
        if self.continuations.len() >= 32 {
            return Err(Error::Cpu("CFM callback depth exceeded".into()));
        }
        crate::lifecycle::Lifecycle::validate_nested(&self.cpu, &self.memory)?;
        Ok(())
    }
    fn enter_fragment(
        &mut self,
        routine: crate::lifecycle::Routine,
        argument: Option<u32>,
        resume: Resume,
    ) -> Result<()> {
        self.validate_fragment_call()?;
        let callee =
            crate::lifecycle::Lifecycle::nested(&self.cpu, &mut self.memory, routine, argument)?;
        let caller = std::mem::replace(&mut self.cpu, callee);
        self.continuations.push(Continuation { caller, resume });
        Ok(())
    }
    fn resume(&mut self, frame: Continuation, value: u32) -> Result<Option<Stop>> {
        self.cpu = frame.caller;
        match frame.resume {
            Resume::Fixture => self.cpu.set_result(value),
            Resume::Loaded {
                connection,
                request,
                reply,
            } => {
                let error = self.process.fragments.initialized(
                    &mut self.memory,
                    &mut self.abi,
                    connection,
                    request,
                    value as i16,
                )?;
                reply.apply(&mut self.cpu, &mut self.memory, error as i32 as u32, error)?;
            }
            Resume::Closed {
                connection,
                destination,
                reply,
            } => {
                self.process.fragments.closed(
                    &mut self.memory,
                    &mut self.abi,
                    connection,
                    destination,
                )?;
                reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
            }
            Resume::Shutdown { connection, stop } => {
                self.process.fragments.closed(
                    &mut self.memory,
                    &mut self.abi,
                    connection,
                    GuestPtr(0),
                )?;
                return self.finish_process(stop);
            }
        }
        Ok(None)
    }
    fn finish_process(&mut self, stop: Stop) -> Result<Option<Stop>> {
        if let Some((connection, routine)) = self
            .process
            .fragments
            .shutdown(&mut self.memory, &mut self.abi)?
        {
            self.enter_fragment(routine, None, Resume::Shutdown { connection, stop })?;
            Ok(None)
        } else {
            self.terminal = Some(stop);
            Ok(Some(stop))
        }
    }
    // Deterministic scheduling policy: 1000 retired boundaries per 60 Hz tick.
    // Host wall time never enters guest time; future hosts pace execution budgets.
    fn tick_count(&self) -> Result<u32> {
        if self.abi.fixture {
            Ok((self.instructions / 1000) as u32)
        } else {
            self.memory.u32(GuestPtr(0x16a))
        }
    }
    fn enter_callback(&mut self, descriptor: GuestPtr) -> Result<()> {
        if self.continuations.len() >= 32 {
            return Err(Error::Cpu("callback depth exceeded".into()));
        }
        // Internal descriptor: ISA (0/1), entry, TOC. Not Apple ProcInfo.
        let bytes = self.memory.read(descriptor, 12)?;
        let word = |i| u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap());
        let arch = match word(0) {
            0 => Architecture::M68k,
            1 => Architecture::PowerPc,
            v => return Err(Error::Fixture(format!("callback ISA {v}"))),
        };
        let entry = word(4);
        let toc = word(8);
        self.memory.fetch(
            GuestPtr(entry),
            if arch == Architecture::M68k { 2 } else { 4 },
            if arch == Architecture::M68k { 2 } else { 4 },
        )?;
        let base = 0x100000 + self.continuations.len() as u32 * 0x2000;
        // Stack mappings are retained and reused at each nesting depth.
        if self.memory.read(GuestPtr(base), 1).is_err() {
            self.memory
                .map(GuestPtr(base), vec![0; 0x2000], Permissions::RW)?;
        }
        let mut callee = Cpu::new(arch, entry, base + 0x1ff0);
        match &mut callee {
            Cpu::M68k(c) => self
                .memory
                .write(GuestPtr(c.dar[15]), &RETURN_GATE.to_be_bytes())?,
            Cpu::PowerPc(c) => c.gpr[2] = toc,
        }
        let caller = std::mem::replace(&mut self.cpu, callee);
        self.continuations.push(Continuation {
            caller,
            resume: Resume::Fixture,
        });
        Ok(())
    }
}
