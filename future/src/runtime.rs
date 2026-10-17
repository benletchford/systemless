use crate::{
    cpu::{Architecture, Call, Cpu, Outcome, RETURN_GATE},
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Budget,
    /// Host must checkpoint the mounted volume and acknowledge before guest execution resumes.
    FlushVolume,
    Waiting {
        ticks: u32,
    },
    Exit(u32),
    Returned(u32),
}
#[derive(Default, Debug)]
pub struct Services {
    pub accumulator: u32,
    /// Successful guest Sound Manager callback returns, for host diagnostics.
    pub completed_sound_callbacks: u64,
    pub completed_timer_callbacks: u64,
    pub completed_vbl_callbacks: u64,
    pub calls: Vec<Call>,
}
// The whole CPU is suspended, retaining even engine-private FPU/control state.
struct Continuation {
    caller: Cpu,
    resume: Resume,
}
enum Resume {
    Fixture,
    Retrace {
        task: GuestPtr,
        waits: Box<SuspendedWaits>,
    },
    Timer {
        waits: Box<SuspendedWaits>,
    },
    Sound {
        channel: u32,
        serial: u64,
        double_generation: Option<u64>,
        waits: Box<SuspendedWaits>,
    },
    Modal {
        wait: Box<ModalWait>,
        result: crate::callback::BooleanResult,
    },
    StandardFile {
        wait: Box<StandardFileWait>,
        result: crate::callback::BooleanResult,
    },
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
struct ResourcePlayWait {
    play: crate::sound::Play,
    reply: crate::abi::Reply,
}
struct SoundWait {
    channel: u32,
    serial: u64,
    command: [u8; 8],
    reply: crate::abi::Reply,
}
struct DelayWait {
    saved_sr: Option<u16>,
    remaining: u32,
    final_ticks: Option<GuestPtr>,
    reply: crate::abi::Reply,
}
struct SuspendedWaits {
    delay: Option<DelayWait>,
    event: Option<EventWait>,
    control: Option<ControlWait>,
    menu: Option<MenuWait>,
    file: Option<StandardFileWait>,
    modal: Option<ModalWait>,
    sound: Option<SoundWait>,
    play: Option<ResourcePlayWait>,
}
struct ModalWait {
    dialog: crate::dialogs::Modal,
    reply: crate::abi::Reply,
}
struct StandardFileWait {
    dialog: crate::standard_file::Dialog,
    reply: crate::abi::Reply,
}
struct MenuWait {
    tracking: crate::menus::Tracking,
    reply: crate::abi::Reply,
}
struct ControlWait {
    tracking: crate::controls::Tracking,
    reply: crate::abi::Reply,
}
struct EventWait {
    region: GuestPtr,
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
    delay_wait: Option<DelayWait>,
    event_wait: Option<EventWait>,
    sound_wait: Option<SoundWait>,
    play_wait: Option<ResourcePlayWait>,
    control_wait: Option<ControlWait>,
    menu_wait: Option<MenuWait>,
    standard_file_wait: Option<StandardFileWait>,
    modal_wait: Option<ModalWait>,
    terminal: Option<Stop>,
    volume_flush: Option<(GuestPtr, crate::abi::Reply)>,
    pub(crate) lifecycle: Option<crate::lifecycle::Lifecycle>,
    faulted: bool,
    calendar: crate::calendar::Clock,
}
impl Runtime {
    pub fn display(&self) -> crate::display::Display {
        self.process.display
    }
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
            delay_wait: None,
            event_wait: None,
            sound_wait: None,
            play_wait: None,
            control_wait: None,
            menu_wait: None,
            standard_file_wait: None,
            modal_wait: None,
            terminal: None,
            volume_flush: None,
            lifecycle: None,
            faulted: false,
            calendar: crate::calendar::Clock::default(),
        }
    }
    /// Initialize the virtual boot-volume environment after optional catalog restore.
    pub fn prepare_application_volume(&mut self) -> Result<()> {
        let error = self.process.files.provision_system_folder(&self.memory)?;
        if error != 0 {
            return Err(Error::Volume(format!(
                "virtual System Folder initialization failed: {error}"
            )));
        }
        Ok(())
    }
    /// Refresh the host display from canonical screen bytes, even after exit.
    /// Offscreen ports and their last drawing texture do not select the display.
    pub fn refresh_screen(&mut self) -> Result<()> {
        let gpu = self
            .graphics
            .as_ref()
            .ok_or_else(|| Error::Graphics("screen refresh requires GPU".into()))?;
        self.process.cursor.observe(self.process.input);
        self.process
            .drivers
            .presentation(self.process.display)?
            .present_draw(
                gpu,
                &self.memory,
                self.process.cursor.draw(self.process.input),
            )
    }
    pub fn volume_flush_pending(&self) -> bool {
        self.volume_flush.is_some()
    }
    /// Acknowledge host persistence (0) or a Macintosh I/O error. Memory-only hosts
    /// explicitly acknowledge their canonical in-memory catalog, not disk durability.
    pub fn complete_volume_flush(&mut self, error: i16) -> Result<()> {
        if self.faulted || self.terminal.is_some() || error > 0 {
            return Err(Error::Cpu("invalid volume flush completion".into()));
        }
        let (pointer, reply) = self
            .volume_flush
            .as_ref()
            .ok_or_else(|| Error::Cpu("no pending volume flush".into()))?;
        let pointer = *pointer;
        let reply = *reply;
        self.memory.validate_write(GuestPtr(pointer.0 + 16), 2)?;
        self.memory
            .write(GuestPtr(pointer.0 + 16), &error.to_be_bytes())?;
        reply.apply(&mut self.cpu, &mut self.memory, error as i32 as u32, error)?;
        self.volume_flush = None;
        Ok(())
    }
    pub fn advance(&mut self, budget: u64) -> Result<Stop> {
        if self.faulted {
            return Err(Error::Cpu("runtime stopped after earlier fault".into()));
        }
        if let Some(stop) = self.terminal {
            return Ok(stop);
        }
        if self.volume_flush.is_some() {
            return Ok(Stop::FlushVolume);
        }
        let result = self.advance_inner(budget).map(|stop| match stop {
            Stop::Waiting { ticks } => Stop::Waiting {
                ticks: self
                    .process
                    .timers
                    .wait_ticks()
                    .map_or(ticks, |timer| ticks.min(timer))
                    .min(self.process.retrace.wait_ticks().unwrap_or(ticks)),
            },
            other => other,
        });
        if result.is_err() {
            self.faulted = true;
        }
        if self.faulted || self.terminal.is_some() {
            self.process.sound.cancel();
            self.process.timers.cancel();
            self.process.retrace.cancel(&mut self.memory);
        }
        result
    }
    /// Choose the guest calendar epoch without changing TickCount. Hosts may set
    /// wall time once; fixtures keep the loader's reproducible epoch.
    pub fn set_date_time(&mut self, seconds: u32) -> Result<()> {
        if self.faulted || self.terminal.is_some() {
            return Err(Error::Cpu("cannot set a stopped process clock".into()));
        }
        self.calendar.set(&mut self.memory, seconds)
    }
    /// Advance the shared sound sample clock without executing guest CPU code.
    pub fn render_sound(&mut self, frames: usize, rate: u32) -> Result<Vec<[i16; 2]>> {
        if self.faulted || self.terminal.is_some() {
            return Err(Error::Sound("cannot render a stopped process".into()));
        }
        let mut sound = std::mem::take(&mut self.process.sound);
        let result = sound.render(&mut self.memory, &mut self.process, frames, rate);
        self.process.sound = sound;
        result
    }
    /// Headless time policy: finish sampled sound during virtual waits without
    /// opening a host device or allocating output for the discarded interval.
    pub fn elapse_sound_silently(&mut self, ticks: u32) -> Result<()> {
        if self.faulted || self.terminal.is_some() {
            return Err(Error::Sound(
                "cannot advance sound in a stopped process".into(),
            ));
        }
        let mut sound = std::mem::take(&mut self.process.sound);
        let result = sound.elapse_silently(&mut self.memory, &mut self.process, ticks);
        self.process.sound = sound;
        result
    }
    /// Host-controlled deterministic time, including periods with no guest CPU
    /// execution. A host can inject events before advancing this clock.
    pub fn elapse_ticks(&mut self, ticks: u32) -> Result<()> {
        self.elapse_ticks_inner(ticks, true)
    }
    fn elapse_ticks_inner(&mut self, ticks: u32, advance_timers: bool) -> Result<()> {
        if self.faulted || self.terminal.is_some() {
            return Err(Error::Cpu("cannot advance a stopped process clock".into()));
        }
        let units = u64::from(ticks) * 50_000;
        if advance_timers {
            self.process.timers.validate_advance(units)?;
        }
        self.calendar.advance(&mut self.memory, ticks)?;
        if advance_timers {
            self.process.timers.advance(units)?;
        }
        for continuation in &mut self.continuations {
            if let Resume::Sound { waits, .. }
            | Resume::Timer { waits }
            | Resume::Retrace { waits, .. } = &mut continuation.resume
            {
                if let Some(wait) = &mut waits.delay {
                    wait.remaining = wait.remaining.saturating_sub(ticks);
                }
                if let Some(wait) = &mut waits.event {
                    wait.remaining = wait.remaining.saturating_sub(ticks);
                }
            }
        }
        if let Some(wait) = &mut self.delay_wait {
            wait.remaining = wait.remaining.saturating_sub(ticks);
        }
        if let Some(wait) = &mut self.event_wait {
            wait.remaining = wait.remaining.saturating_sub(ticks);
        }
        Ok(())
    }
    fn advance_inner(&mut self, budget: u64) -> Result<Stop> {
        if budget == 0 {
            return Ok(Stop::Budget);
        }
        for _ in 0..budget {
            self.enter_pending_sound_callback()?;
            self.enter_pending_timer_callback()?;
            self.enter_pending_retrace_callback()?;
            if let Some(stop) = self.poll_waits()? {
                return Ok(stop);
            }
            self.instructions = self
                .instructions
                .checked_add(1)
                .ok_or_else(|| Error::Cpu("instruction clock overflow".into()))?;
            if !self.abi.fixture {
                self.process.timers.advance(50)?;
                if self.instructions.is_multiple_of(1000) {
                    self.elapse_ticks_inner(1, false)?;
                }
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
                    if self
                        .continuations
                        .iter()
                        .any(|c| matches!(c.resume, Resume::Retrace { .. }))
                        && !matches!(
                            call,
                            Call::Ticks
                                | Call::SecondsToDate { .. }
                                | Call::ReadDateTime(_)
                                | Call::PostEvent { .. }
                                | Call::FixedMultiply { .. }
                                | Call::LongToFixed(_)
                                | Call::FixedRatio { .. }
                                | Call::GetMmuMode
                                | Call::SwapMmuMode(_)
                                | Call::QuickDraw32Addressing
                                | Call::Logical { .. }
                                | Call::EmptyRect(_)
                                | Call::PointInRect { .. }
                                | Call::Gestalt(_)
                                | Call::Timer(_)
                                | Call::Retrace(_)
                                | Call::RetraceHeader
                        )
                    {
                        return Err(Error::Cpu("service is unsupported at VBL interrupt time (no allocation or blocking)".into()));
                    }
                    match call {
                        Call::DrawOval { rectangle, op } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("oval drawing requires InitGraf".into())
                            })?;
                            let r = crate::quickdraw::read_rect(&self.memory, rectangle)?;
                            ports.draw_oval(self.graphics.as_ref(), &mut self.memory, r, op)?;
                        }
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
                        Call::RgbColor {
                            background,
                            pointer,
                        } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("RGBColor requires InitGraf".into())
                            })?;
                            ports.set_rgb(&mut self.memory, background, pointer)?;
                        }
                        Call::GetRgbColor {
                            background,
                            pointer,
                        } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("GetRGBColor requires InitGraf".into())
                            })?;
                            ports.get_rgb(&mut self.memory, background, pointer)?;
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
                        Call::StandardPutFile(request) => {
                            let gpu = self.graphics.as_ref().ok_or_else(|| {
                                Error::Graphics("Standard File requires a GPU".into())
                            })?;
                            let ticks = self.tick_count()?;
                            let dialog = crate::standard_file::Dialog::start(
                                &mut self.memory,
                                &mut self.process,
                                gpu,
                                request,
                                ticks,
                            )?;
                            if let Some(stop) =
                                self.poll_standard_file(StandardFileWait { dialog, reply })?
                            {
                                return Ok(stop);
                            }
                        }
                        Call::StandardGetFile(request) => {
                            let gpu = self.graphics.as_ref().ok_or_else(|| {
                                Error::Graphics("Standard File requires a GPU".into())
                            })?;
                            let ticks = self.tick_count()?;
                            let dialog = crate::standard_file::Dialog::start_open(
                                &mut self.memory,
                                &mut self.process,
                                gpu,
                                request,
                                ticks,
                            )?;
                            if let Some(stop) =
                                self.poll_standard_file(StandardFileWait { dialog, reply })?
                            {
                                return Ok(stop);
                            }
                        }
                        Call::TrackControl(request) => {
                            if let crate::abi::Reply::StackWord(p) = reply {
                                self.memory.validate_write(p, 2)?;
                            }
                            let mut controls = std::mem::take(&mut self.process.controls);
                            let result = controls.begin_tracking(
                                &mut self.memory,
                                &mut self.process,
                                request,
                            );
                            self.process.controls = controls;
                            if let Some(tracking) = result? {
                                if let Some(stop) =
                                    self.poll_control_wait(ControlWait { tracking, reply })?
                                {
                                    return Ok(stop);
                                }
                            } else {
                                reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                            }
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
                        | Call::NewColorWindow(_)
                        | Call::WindowChange { .. }
                        | Call::ResourceWindow(_)
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
                                Call::ResourceWindow(request) => windows.from_resource(
                                    &mut self.memory,
                                    &mut self.process,
                                    gpu,
                                    request,
                                ),
                                Call::NewColorWindow(request) => windows.create_color(
                                    &mut self.memory,
                                    &mut self.process,
                                    gpu,
                                    request,
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
                            let shown = match call {
                                Call::Window {
                                    op: crate::windows::Op::Show,
                                    pointer,
                                } => Some(pointer),
                                Call::WindowChange {
                                    pointer,
                                    change: crate::windows::Change::Visibility(true),
                                } => Some(pointer),
                                _ => None,
                            };
                            if let Some(dialog) = shown {
                                let mut dialogs = std::mem::take(&mut self.process.dialogs);
                                let result = dialogs.show_default(
                                    &mut self.memory,
                                    &mut self.process,
                                    gpu,
                                    dialog,
                                );
                                self.process.dialogs = dialogs;
                                result?;
                            }
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
                        Call::WindowRepaint(request) => {
                            let gpu = self.graphics.as_ref().ok_or_else(|| {
                                Error::Graphics("window repaint requires a GPU".into())
                            })?;
                            let windows = std::mem::take(&mut self.process.windows);
                            let result =
                                windows.repaint(&mut self.memory, &mut self.process, gpu, request);
                            self.process.windows = windows;
                            result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::WindowVisibility(request) => {
                            let windows = std::mem::take(&mut self.process.windows);
                            let result =
                                windows.visibility(&mut self.memory, &mut self.process, request);
                            self.process.windows = windows;
                            result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::RegionRecording(op) => {
                            let mut manager = std::mem::take(&mut self.process.region_recording);
                            let result = manager.call(&mut self.memory, &mut self.process, op);
                            self.process.region_recording = manager;
                            result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::Polygon(op) => {
                            let mut manager = std::mem::take(&mut self.process.polygons);
                            let result = manager.call(
                                &mut self.memory,
                                &mut self.process,
                                self.graphics.as_ref(),
                                op,
                            );
                            self.process.polygons = manager;
                            let value = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::PenVisibility(delta) => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            ports.pen_visibility(&mut self.memory, delta)?;
                        }
                        Call::PenLine { h, v, relative } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            let mut manager = std::mem::take(&mut self.process.polygons);
                            let result = manager.line(
                                &mut self.memory,
                                &mut self.process,
                                self.graphics.as_ref(),
                                h,
                                v,
                                relative,
                            );
                            self.process.polygons = manager;
                            if result? {
                                self.memory.write(
                                    GuestPtr(0x220),
                                    &self.process.mem_error.to_be_bytes(),
                                )?;
                            } else {
                                ports.line(
                                    &mut self.memory,
                                    self.graphics.as_ref(),
                                    h,
                                    v,
                                    relative,
                                )?;
                            }
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
                        Call::Gestalt(selector) => {
                            let (value, error) = match crate::environment::gestalt(selector) {
                                Ok(value) => (value, 0),
                                Err(error) => (0, error),
                            };
                            reply.apply(&mut self.cpu, &mut self.memory, value, error)?;
                        }
                        Call::SecondsToDate {
                            seconds,
                            destination,
                        } => {
                            crate::calendar::seconds_to_date(
                                &mut self.memory,
                                seconds,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ReadDateTime(destination) => {
                            crate::calendar::read(&mut self.memory, destination)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::Decimal(op) => {
                            let result = crate::decimal::call(&mut self.memory, op)?;
                            reply.apply(&mut self.cpu, &mut self.memory, result, 0)?;
                        }
                        Call::Button => {
                            // Apple TN DV520: polling reads the current MBState high bit.
                            let pressed = crate::events::button(&self.memory)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(pressed), 0)?;
                        }
                        Call::StillDown => {
                            let down = crate::events::still_down(&self.memory)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(down), 0)?;
                        }
                        Call::FindWindow { point, destination } => {
                            let windows = std::mem::take(&mut self.process.windows);
                            let result = windows.find(
                                &mut self.memory,
                                &mut self.process,
                                point,
                                destination,
                            );
                            self.process.windows = windows;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                result? as i32 as u32,
                                0,
                            )?;
                        }
                        Call::EmptyRect(rectangle) => {
                            let value =
                                u32::from(crate::rectangles::empty(&self.memory, rectangle)?);
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::PointInRect { point, rectangle } => {
                            let inside =
                                crate::rectangles::contains(&self.memory, point, rectangle)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(inside), 0)?;
                        }
                        Call::GetWMgrPort(destination) => {
                            self.memory.validate_write(destination, 4)?;
                            let port = self.process.windows.manager_port(&self.memory)?;
                            self.memory.write(destination, &port.to_be_bytes())?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::GetKeys(destination) => {
                            crate::keyboard::get_keys(&mut self.memory, destination)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
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
                            let result = crate::environment::query_profile(
                                &mut self.memory,
                                version,
                                record,
                                self.process.display,
                                self.process.files.system_volume_reference(),
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                result as i32 as u32,
                                0,
                            )?;
                        }
                        Call::ColorPort { action, port } => {
                            let mut ports = std::mem::take(&mut self.process.color_ports);
                            let result =
                                ports.call(&mut self.memory, &mut self.process, action, port);
                            self.process.color_ports = ports;
                            result?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::IntegerToExtended {
                            source,
                            destination,
                            bits,
                        } => {
                            self.process.numerics.integer_to_extended(
                                &mut self.memory,
                                source,
                                destination,
                                bits,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::DivideExtended {
                            source,
                            destination,
                        } => {
                            self.process.numerics.divide_extended(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::SingleToExtended {
                            source,
                            destination,
                        } => {
                            self.process.numerics.single_to_extended(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::DoubleToExtended {
                            source,
                            destination,
                        } => {
                            self.process.numerics.double_to_extended(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ExtendedToInteger {
                            source,
                            destination,
                            bits,
                        } => {
                            self.process.numerics.extended_to_integer(
                                &mut self.memory,
                                source,
                                destination,
                                bits,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ExtendedToSingle {
                            source,
                            destination,
                        } => {
                            self.process.numerics.extended_to_single(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ExtendedToDouble {
                            source,
                            destination,
                        } => {
                            self.process.numerics.extended_to_double(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::MultiplyExtended {
                            source,
                            destination,
                        } => {
                            self.process.numerics.multiply_extended(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::SignExtended { pointer, absolute } => {
                            self.process.numerics.sign_extended(
                                &mut self.memory,
                                pointer,
                                absolute,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::RoundIntegralExtended { pointer, truncate } => {
                            self.process.numerics.round_integral_extended(
                                &mut self.memory,
                                pointer,
                                truncate,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::PowerExtended {
                            source,
                            destination,
                        } => {
                            self.process.numerics.power_extended(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::NumericBinary {
                            source,
                            destination,
                            format,
                            op,
                        } => {
                            self.process.numerics.binary(
                                &mut self.memory,
                                source,
                                destination,
                                format,
                                op,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::SquareRootExtended(pointer) => {
                            self.process
                                .numerics
                                .square_root_extended(&mut self.memory, pointer)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::AddExtended {
                            source,
                            destination,
                            subtract,
                        } => {
                            self.process.numerics.add_extended(
                                &mut self.memory,
                                source,
                                destination,
                                subtract,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::NumericCompare {
                            source,
                            destination,
                            format,
                            signaling,
                        } => {
                            let relation = self.process.numerics.compare(
                                &self.memory,
                                source,
                                destination,
                                format,
                                signaling,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, relation as u32, 0)?;
                        }
                        Call::NumericEnvironment { pointer, op } => {
                            self.process
                                .numerics
                                .environment(&mut self.memory, pointer, op)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ClassifyExtended {
                            source,
                            destination,
                        } => {
                            self.process.numerics.classify_extended(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::CopyExtended {
                            source,
                            destination,
                        } => {
                            self.process.numerics.copy_extended(
                                &mut self.memory,
                                source,
                                destination,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::Elementary { pointer, op } => {
                            self.process
                                .numerics
                                .elementary(&mut self.memory, pointer, op)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::AtanExtended(pointer) => {
                            self.process
                                .numerics
                                .atan_extended(&mut self.memory, pointer)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::Color(op) => {
                            let mut manager = std::mem::take(&mut self.process.color);
                            let result = manager.call(&mut self.memory, &mut self.process, op);
                            self.process.color = manager;
                            reply.apply(&mut self.cpu, &mut self.memory, result?, 0)?;
                        }
                        Call::Device(op) => {
                            let mut devices = std::mem::take(&mut self.process.devices);
                            let result = devices.call(&mut self.memory, &mut self.process, op);
                            self.process.devices = devices;
                            reply.apply(&mut self.cpu, &mut self.memory, result?, 0)?;
                        }
                        Call::MoreMasters => {
                            self.process.more_masters(&mut self.memory)?;
                            let error = self.process.mem_error;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::MaxApplZone => {
                            self.process.maximize_heap(&mut self.memory)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::SetApplLimit(limit) => {
                            self.process
                                .set_application_limit(&mut self.memory, limit)?;
                            let error = self.process.mem_error;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
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
                        Call::Dialog(crate::dialogs::Op::Modal { filter, item_hit }) => {
                            if self.graphics.is_none() {
                                return Err(Error::Graphics("ModalDialog requires a GPU".into()));
                            }
                            let manager = std::mem::take(&mut self.process.dialogs);
                            let result = manager.begin_modal(
                                &mut self.memory,
                                &mut self.process,
                                filter,
                                item_hit,
                            );
                            self.process.dialogs = manager;
                            let dialog = result?;
                            if let Some(stop) = self.poll_modal(ModalWait { dialog, reply })? {
                                return Ok(stop);
                            }
                        }
                        Call::Dialog(op) => {
                            let value = if matches!(
                                op,
                                crate::dialogs::Op::Resource { .. }
                                    | crate::dialogs::Op::Text { .. }
                                    | crate::dialogs::Op::Draw(_)
                                    | crate::dialogs::Op::SelectText { .. }
                                    | crate::dialogs::Op::DefaultItem { .. }
                                    | crate::dialogs::Op::Item { .. }
                                    | crate::dialogs::Op::Dispose { .. }
                            ) {
                                if !matches!(
                                    op,
                                    crate::dialogs::Op::Item { .. }
                                        | crate::dialogs::Op::Text { .. }
                                        | crate::dialogs::Op::Draw(_)
                                        | crate::dialogs::Op::SelectText { .. }
                                        | crate::dialogs::Op::DefaultItem { .. }
                                ) && self.graphics.is_none()
                                {
                                    return Err(Error::Graphics(
                                        "dialog operation requires a GPU".into(),
                                    ));
                                }
                                let mut manager = std::mem::take(&mut self.process.dialogs);
                                let result = match op {
                                    crate::dialogs::Op::Text {
                                        handle,
                                        pointer,
                                        set,
                                    } => manager
                                        .item_text(
                                            &mut self.memory,
                                            &mut self.process,
                                            self.graphics.as_ref(),
                                            handle,
                                            pointer,
                                            set,
                                        )
                                        .map(|_| 0),
                                    crate::dialogs::Op::SelectText {
                                        dialog,
                                        number,
                                        start,
                                        end,
                                    } => manager
                                        .select_text(
                                            &mut self.memory,
                                            &mut self.process,
                                            self.graphics.as_ref(),
                                            dialog,
                                            number,
                                            (start, end),
                                        )
                                        .map(|_| 0),
                                    crate::dialogs::Op::Draw(dialog) => manager
                                        .draw(
                                            &mut self.memory,
                                            &mut self.process,
                                            self.graphics.as_ref(),
                                            dialog,
                                        )
                                        .map(|_| 0),
                                    crate::dialogs::Op::DefaultItem { dialog, number } => manager
                                        .default_item(
                                            &mut self.memory,
                                            &mut self.process,
                                            self.graphics.as_ref(),
                                            dialog,
                                            number,
                                        )
                                        .map(|v| v as i32 as u32),
                                    crate::dialogs::Op::Resource {
                                        id,
                                        storage,
                                        behind,
                                    } => manager.resource(
                                        &mut self.memory,
                                        &mut self.process,
                                        self.graphics
                                            .as_ref()
                                            .expect("GPU checked before taking dialog manager"),
                                        id,
                                        storage,
                                        behind,
                                    ),
                                    crate::dialogs::Op::Item {
                                        dialog,
                                        number,
                                        kind,
                                        handle,
                                        bounds,
                                    } => manager
                                        .item(
                                            &mut self.memory,
                                            &mut self.process,
                                            dialog,
                                            number,
                                            [kind, handle, bounds],
                                        )
                                        .map(|_| 0),
                                    crate::dialogs::Op::Dispose { dialog, close } => manager
                                        .dispose(
                                            &mut self.memory,
                                            &mut self.process,
                                            self.graphics
                                                .as_ref()
                                                .expect("GPU checked before taking dialog manager"),
                                            dialog,
                                            close,
                                        )
                                        .map(|_| 0),
                                    _ => unreachable!(),
                                };
                                self.process.dialogs = manager;
                                result?
                            } else if let crate::dialogs::Op::Slot(index) = op {
                                crate::dialogs::slot(&self.memory, index)?
                            } else if let crate::dialogs::Op::IsEvent(event) = op {
                                u32::from(
                                    self.process.windows.is_dialog_event(&self.memory, event)?,
                                )
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
                        Call::FixedMultiply { left, right } => {
                            let value = crate::integer::fixed_multiply(left, right) as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::LongToFixed(input) => {
                            let value = crate::integer::long_to_fixed(input) as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::FixedRatio {
                            numerator,
                            denominator,
                        } => {
                            let value = crate::integer::fixed_ratio(numerator, denominator) as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::Word { value, high } => {
                            let value = crate::integer::word(value, high) as i32 as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::MenuChoice => {
                            let value = self.memory.u32(GuestPtr(0xb54))?;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::HiliteMenu(id) => {
                            let gpu = self.graphics.as_ref().ok_or_else(|| {
                                Error::Graphics("HiliteMenu requires a GPU".into())
                            })?;
                            let manager = std::mem::take(&mut self.process.menus);
                            let result =
                                manager.hilite(&mut self.memory, &mut self.process, gpu, id);
                            self.process.menus = manager;
                            result?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::MenuSelect(point) => {
                            let gpu = self.graphics.as_ref().ok_or_else(|| {
                                Error::Graphics("MenuSelect requires a GPU".into())
                            })?;
                            let manager = std::mem::take(&mut self.process.menus);
                            let result = manager.begin_tracking(
                                &mut self.memory,
                                &mut self.process,
                                gpu,
                                point,
                            );
                            self.process.menus = manager;
                            if let Some(tracking) = result? {
                                if let Some(stop) =
                                    self.poll_menu_wait(MenuWait { tracking, reply })?
                                {
                                    return Ok(stop);
                                }
                            } else {
                                reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                            }
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
                        Call::Logical { op, left, right } => {
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                op.apply(left, right),
                                0,
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
                            self.process.ports = Some(crate::quickdraw::initialize_display(
                                &mut self.memory,
                                slot,
                                anchor,
                                self.process.display,
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
                                    crate::port_layout::write_pattern(
                                        &mut self.memory,
                                        p,
                                        crate::port_layout::Pattern::Pen,
                                        &pattern,
                                    )?;
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
                        Call::CopyMask(request) => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("CopyMask requires InitGraf".into())
                            })?;
                            let gpu = self
                                .graphics
                                .as_ref()
                                .ok_or_else(|| Error::Graphics("CopyMask requires a GPU".into()))?;
                            ports.copy_mask(&mut self.memory, gpu, request)?;
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
                        Call::PlotCIcon {
                            handle,
                            destination,
                        } => {
                            if handle != 0 && destination.0 != 0 {
                                let ports = self.process.ports.ok_or_else(|| {
                                    Error::Graphics("PlotCIcon requires InitGraf".into())
                                })?;
                                let gpu = self.graphics.as_ref().ok_or_else(|| {
                                    Error::Graphics("PlotCIcon requires a GPU".into())
                                })?;
                                crate::cicon::plot(
                                    &mut self.memory,
                                    &mut self.process,
                                    ports,
                                    gpu,
                                    handle,
                                    destination,
                                )?;
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
                        Call::SetOrigin { h, v } => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("SetOrigin requires InitGraf".into())
                            })?;
                            ports.set_origin(&mut self.memory, &mut self.process, h, v)?;
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
                        Call::PenNormal => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            ports.pen_normal(&mut self.memory)?;
                        }
                        Call::PenMode(mode) => {
                            let ports = self.process.ports.ok_or_else(|| {
                                Error::Graphics("QuickDraw has not been initialized".into())
                            })?;
                            let p = ports.current(&self.memory)?;
                            self.memory.write(GuestPtr(p.0 + 56), &mode.to_be_bytes())?;
                        }
                        Call::Delay { ticks, final_ticks } => {
                            self.tick_count()?;
                            if let Some(p) = final_ticks {
                                if p.0 == 0 {
                                    return Err(Error::Cpu(
                                        "nil Delay finalTicks destination".into(),
                                    ));
                                }
                                self.memory.validate_write(p, 4)?;
                            }
                            let saved_sr = if let Cpu::M68k(c) = &mut self.cpu {
                                let sr = c.get_sr();
                                c.set_sr(sr & !0x0700);
                                Some(sr)
                            } else {
                                None
                            };
                            let wait = DelayWait {
                                saved_sr,
                                remaining: ticks,
                                final_ticks,
                                reply,
                            };
                            if let Some(stop) = self.poll_delay(wait)? {
                                return Ok(stop);
                            }
                        }
                        Call::WaitEvent {
                            mask,
                            destination,
                            sleep,
                            region,
                        } => {
                            self.memory.validate_write(destination, 16)?;
                            let wait = EventWait {
                                region,
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
                                return Err(Error::Cpu(
                                    "process exit during guest callback or CFM init/term".into(),
                                ));
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
                        Call::CopyHandle(handle) => {
                            let value = self.process.copy_handle(&mut self.memory, handle)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value,
                                self.process.mem_error,
                            )?;
                        }
                        Call::MoveHandleHigh(handle) => {
                            self.process.move_handle_high(&mut self.memory, handle)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::HandleState { handle, value } => {
                            let flags = if let Some(flags) = value {
                                self.process.set_handle_state(&self.memory, handle, flags)?;
                                0
                            } else {
                                self.process.handle_state(&self.memory, handle)? as i8 as i32 as u32
                            };
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                flags,
                                self.process.mem_error,
                            )?;
                        }
                        Call::HandleAttribute {
                            handle,
                            resource,
                            enabled,
                        } => {
                            self.process.set_handle_attribute(
                                &self.memory,
                                handle,
                                resource,
                                enabled,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
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
                        Call::WriteResource(handle) => {
                            self.process.write_resource(&self.memory, handle)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::AddResource(request) => {
                            self.process.add_resource(&self.memory, request)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::CreateSpecResource(request) => {
                            self.memory.validate_write(GuestPtr(0xa60), 2)?;
                            self.process.res_error = self
                                .process
                                .files
                                .create_spec_resource(&self.memory, request)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::CreateResourceFile(name) => {
                            self.memory.validate_write(GuestPtr(0xa60), 2)?;
                            self.process.res_error = self
                                .process
                                .files
                                .create_resource_file(&self.memory, name)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ResourceInfo(request) => {
                            self.memory.validate_write(GuestPtr(0xa60), 2)?;
                            self.process.resource_info(&mut self.memory, request)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::CIcon(op) => {
                            self.memory.validate_write(GuestPtr(0x220), 2)?;
                            if matches!(op, crate::cicon::Op::Get(_)) {
                                self.memory.validate_write(GuestPtr(0xa60), 2)?;
                            }
                            let mut manager = std::mem::take(&mut self.process.cicons);
                            let result = manager.call(&mut self.memory, &mut self.process, op);
                            self.process.cicons = manager;
                            reply.apply(&mut self.cpu, &mut self.memory, result?, 0)?;
                        }
                        Call::PixPat(op) => {
                            self.memory.validate_write(GuestPtr(0x220), 2)?;
                            if matches!(op, crate::pixpat::Op::Get(_)) {
                                self.memory.validate_write(GuestPtr(0xa60), 2)?;
                            }
                            let mut manager = std::mem::take(&mut self.process.pixpats);
                            let result = manager.call(&mut self.memory, &mut self.process, op);
                            self.process.pixpats = manager;
                            reply.apply(&mut self.cpu, &mut self.memory, result?, 0)?;
                        }
                        Call::GWorld(op) => {
                            self.memory.validate_write(GuestPtr(0x220), 2)?;
                            let mut manager = std::mem::take(&mut self.process.gworlds);
                            let result = manager.call(
                                &mut self.memory,
                                &mut self.process,
                                op,
                                self.graphics.as_ref(),
                            );
                            self.process.gworlds = manager;
                            reply.apply(&mut self.cpu, &mut self.memory, result?, 0)?;
                        }
                        Call::PixelState { pixmap, action } => {
                            let state =
                                self.process
                                    .pixels_state(&mut self.memory, pixmap, action)?;
                            reply.apply(&mut self.cpu, &mut self.memory, state, 0)?;
                        }
                        Call::PixelBase(pixmap) => {
                            let base = self.process.pixel_base(&self.memory, pixmap)?;
                            reply.apply(&mut self.cpu, &mut self.memory, base, 0)?;
                        }
                        Call::Pixels { pixmap, lock } => {
                            let result = self.process.pixels(&mut self.memory, pixmap, lock)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(result), 0)?;
                        }
                        Call::CountResources { kind, current_only } => {
                            self.memory.validate_write(GuestPtr(0xa60), 2)?;
                            let count = self.process.count_resources(kind, current_only)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(count), 0)?;
                        }
                        Call::GetIndexedResource {
                            kind,
                            index,
                            current_only,
                        } => {
                            self.memory.validate_write(GuestPtr(0xa60), 2)?;
                            self.memory.validate_write(GuestPtr(0x220), 2)?;
                            let value = self.process.get_indexed_resource(
                                &mut self.memory,
                                kind,
                                index,
                                current_only,
                            )?;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::GetCurrentResource { kind, id } => {
                            let v =
                                self.process
                                    .get_current_resource(&mut self.memory, kind, id)?;
                            reply.apply(&mut self.cpu, &mut self.memory, v, 0)?;
                        }
                        Call::GetResource { kind, id } => {
                            let v = self.process.get_resource(&mut self.memory, kind, id)?;
                            reply.apply(&mut self.cpu, &mut self.memory, v, 0)?;
                        }
                        Call::SoundVersion => reply.apply(
                            &mut self.cpu,
                            &mut self.memory,
                            crate::sound::VERSION,
                            0,
                        )?,
                        Call::SoundStatus(request) => {
                            let sound = std::mem::take(&mut self.process.sound);
                            let result = sound.status(&mut self.memory, &mut self.process, request);
                            self.process.sound = sound;
                            let error = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::SoundDouble { channel, header } => {
                            let mut sound = std::mem::take(&mut self.process.sound);
                            let result = sound.play_double(
                                &mut self.memory,
                                &mut self.process,
                                channel,
                                header,
                            );
                            self.process.sound = sound;
                            let error = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::SoundPauseFile(channel) => {
                            let mut sound = std::mem::take(&mut self.process.sound);
                            let result = sound.pause_file(&self.memory, &mut self.process, channel);
                            self.process.sound = sound;
                            let error = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::SoundStartFile(request) => {
                            let mut sound = std::mem::take(&mut self.process.sound);
                            let result =
                                sound.begin_file_play(&mut self.memory, &mut self.process, request);
                            self.process.sound = sound;
                            match result? {
                                Err(error) => reply.apply(
                                    &mut self.cpu,
                                    &mut self.memory,
                                    error as i32 as u32,
                                    error,
                                )?,
                                Ok(play) => {
                                    if let Some(stop) =
                                        self.poll_resource_play(ResourcePlayWait { play, reply })?
                                    {
                                        return Ok(stop);
                                    }
                                }
                            }
                        }
                        Call::SoundPlay {
                            channel,
                            handle,
                            asynchronous,
                        } => {
                            let mut sound = std::mem::take(&mut self.process.sound);
                            let result = sound.begin_play(
                                &mut self.memory,
                                &mut self.process,
                                channel,
                                handle,
                                asynchronous,
                            );
                            self.process.sound = sound;
                            match result? {
                                Err(error) => reply.apply(
                                    &mut self.cpu,
                                    &mut self.memory,
                                    error as i32 as u32,
                                    error,
                                )?,
                                Ok(play) => {
                                    if let Some(stop) =
                                        self.poll_resource_play(ResourcePlayWait { play, reply })?
                                    {
                                        return Ok(stop);
                                    }
                                }
                            }
                        }
                        Call::SoundQueue {
                            channel,
                            command,
                            no_wait,
                        } => {
                            if command.0 == 0 {
                                return Err(Error::Sound("nil queued sound command".into()));
                            }
                            let bytes = self.memory.read(command, 8)?.try_into().unwrap();
                            let wait = SoundWait {
                                channel,
                                serial: self.process.sound.generation(channel),
                                command: bytes,
                                reply,
                            };
                            if let Some(stop) = self.poll_sound_wait(wait)? {
                                if no_wait {
                                    let wait = self.sound_wait.take().unwrap();
                                    wait.reply.apply(
                                        &mut self.cpu,
                                        &mut self.memory,
                                        (-203i32) as u32,
                                        -203,
                                    )?;
                                } else {
                                    return Ok(stop);
                                }
                            }
                        }
                        Call::SoundImmediate { channel, command } => {
                            let mut sound = std::mem::take(&mut self.process.sound);
                            let result = sound.immediate(
                                &mut self.memory,
                                &mut self.process,
                                channel,
                                command,
                            );
                            self.process.sound = sound;
                            let error = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::SoundChannel(op) => {
                            let mut sound = std::mem::take(&mut self.process.sound);
                            let result = sound.call(&mut self.memory, &mut self.process, op);
                            self.process.sound = sound;
                            let error = result?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::EqualString(request) => {
                            let equal = request.evaluate(&self.memory)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(equal), 0)?;
                        }
                        Call::GetOutputVolume(destination) => {
                            let error = self
                                .process
                                .sound
                                .get_output_volume(&mut self.memory, destination)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::SetOutputVolume(level) => {
                            let error = self
                                .process
                                .sound
                                .set_output_volume(&mut self.memory, level)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
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
                            let error = self.process.drivers.control(
                                &mut self.memory,
                                self.process.display,
                                pointer,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::GetMmuMode => {
                            let mode = crate::addressing::current(&self.memory)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(mode), 0)?;
                        }
                        Call::SwapMmuMode(requested) => {
                            let previous = crate::addressing::swap(&self.memory, requested)?;
                            reply.apply(&mut self.cpu, &mut self.memory, u32::from(previous), 0)?;
                        }
                        Call::QuickDraw32Addressing => {
                            // Apple's Jackson Pollock product plan (1989, scanned
                            // page 38) specifies an RTS for this internal address
                            // entry on native 32-bit machines. Preserve every
                            // register/flag/stack byte; do not guess a 24-bit ABI.
                            crate::addressing::current(&self.memory)?;
                        }
                        Call::StripAddress(address) => {
                            // This runtime starts in 32-bit mode. IM: Memory
                            // Utilities specifies an unchanged address in that mode.
                            reply.apply(&mut self.cpu, &mut self.memory, address, 0)?;
                        }
                        Call::MaxMemory(grow) => {
                            self.memory.validate_write(GuestPtr(0x220), 2)?;
                            if let Some(output) = grow {
                                self.memory.validate_write(output, 4)?;
                            }
                            let value = self.process.max_memory(&mut self.memory)?;
                            if let Some(output) = grow {
                                self.memory.write(output, &0u32.to_be_bytes())?;
                            }
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::FreeMemory => {
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                self.process.free_memory(),
                                0,
                            )?;
                        }
                        Call::Munger(request) => {
                            let value = self.process.munger(&mut self.memory, request)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                value as u32,
                                self.process.mem_error,
                            )?;
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
                        Call::FindFolder(request) => {
                            let error =
                                self.process.files.find_folder(&mut self.memory, request)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::VolumeInfo {
                            pointer,
                            hierarchical,
                        } => {
                            let error = self.process.files.volume_info(
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
                        Call::FileIo { pointer, op } => {
                            let error = self.process.files.io(&mut self.memory, pointer, op)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::WorkingDirectory { pointer, op } => {
                            let error = self.process.files.working_directory(
                                &mut self.memory,
                                pointer,
                                op,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::FlushVolume(pointer) => {
                            let error = self.process.files.prepare_flush(&self.memory, pointer)?;
                            self.memory.write(GuestPtr(pointer.0 + 12), &[0; 4])?;
                            self.memory.write(
                                GuestPtr(pointer.0 + 16),
                                &if error == 0 { 1i16 } else { error }.to_be_bytes(),
                            )?;
                            if error == 0 {
                                self.volume_flush = Some((pointer, reply));
                                return Ok(Stop::FlushVolume);
                            }
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::SetSpecInfo(request) => {
                            let error = self.process.files.set_spec_info(&self.memory, request)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::GetSpecInfo(request) => {
                            let error = self
                                .process
                                .files
                                .get_spec_info(&mut self.memory, request)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::DeleteSpec(spec) => {
                            let error = self.process.files.delete_spec(&self.memory, spec)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::MakeSpec(request) => {
                            let error = self.process.files.make_spec(&mut self.memory, request)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::NewAlias(request) => {
                            let error = crate::files::new_alias(
                                &mut self.memory,
                                &mut self.process,
                                request,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::ResolveAlias(request) => {
                            let error = crate::files::resolve_alias(
                                &mut self.memory,
                                &mut self.process,
                                request,
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::ResolveAliasFile(request) => {
                            let error = self
                                .process
                                .files
                                .resolve_alias_file(&mut self.memory, request)?;
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
                        Call::RetraceHeader => {
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                crate::retrace::SYSTEM.0,
                                0,
                            )?;
                        }
                        Call::Retrace(op) => {
                            if !op.remove && self.process.timers.overlaps(op.task) {
                                return Err(Error::Cpu(
                                    "VBL task overlaps a managed Time Manager record".into(),
                                ));
                            }
                            let error = self.process.retrace.call(
                                &mut self.memory,
                                op,
                                self.process.timers.now(),
                            )?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                error as i32 as u32,
                                error,
                            )?;
                        }
                        Call::Timer(op) => {
                            if op.action == crate::timers::Action::Insert
                                && self.process.retrace.overlaps(op.task)
                            {
                                return Err(Error::Cpu(
                                    "Time Manager task overlaps a managed VBL record".into(),
                                ));
                            }
                            self.process.timers.call(&mut self.memory, op)?;
                            reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                        }
                        Call::ResourceFile(op) => {
                            let value = self.process.resource_file(&mut self.memory, op)?;
                            reply.apply(&mut self.cpu, &mut self.memory, value as i32 as u32, 0)?;
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
                    if !matches!(
                        call,
                        Call::Region {
                            op: crate::regions::Op::Clear | crate::regions::Op::Offset,
                            ..
                        } | Call::Menu(crate::menus::Op::Enable { .. })
                            | Call::Control(crate::controls::Op::Geometry { .. })
                            | Call::TextEdit(crate::textedit::Op::Height { .. })
                    ) && matches!(
                        call,
                        Call::ColorPort { .. }
                            | Call::SetApplLimit(_)
                            | Call::MoreMasters
                            | Call::MaxApplZone
                            | Call::MaxMemory(_)
                            | Call::Dialog(
                                crate::dialogs::Op::Resource { .. }
                                    | crate::dialogs::Op::SelectText { .. }
                                    | crate::dialogs::Op::Text { set: true, .. }
                                    | crate::dialogs::Op::Dispose { .. }
                            )
                            | Call::Dialog(
                                crate::dialogs::Op::Init(_) | crate::dialogs::Op::Param(_)
                            )
                            | Call::Menu(_)
                            | Call::WindowRepaint(_)
                            | Call::WindowVisibility(_)
                            | Call::RegionRecording(_)
                            | Call::Polygon(_)
                            | Call::TextEdit(_)
                            | Call::TextDraw(_)
                            | Call::TextWidth(_)
                            | Call::InitFonts
                            | Call::FontInfo(_)
                            | Call::ResourceWindow(_)
                            | Call::NewControl(_)
                            | Call::TrackControl(_)
                            | Call::Control(_)
                            | Call::WindowInvalidation { .. }
                            | Call::WindowChange { .. }
                            | Call::NewWindow(_)
                            | Call::NewColorWindow(_)
                            | Call::Window { .. }
                            | Call::NewPointer { .. }
                            | Call::PointerSize(_)
                            | Call::CIcon(_)
                            | Call::PixPat(_)
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
                            | Call::HandleState { .. }
                            | Call::CopyHandle(_)
                            | Call::Device(crate::devices::Op::GetTable(_))
                            | Call::MoveHandleHigh(_)
                            | Call::HandleAttribute { .. }
                            | Call::DisposeHandle(_)
                            | Call::CombineRegions { .. }
                            | Call::Region { .. }
                            | Call::HandleSize(_)
                            | Call::SetHandleSize { .. }
                            | Call::NewAlias(_)
                            | Call::ResolveAlias(_)
                            | Call::ResourceFile(
                                crate::process::resources::Op::Open { .. }
                                    | crate::process::resources::Op::OpenSpec { .. }
                            )
                            | Call::GetIndexedResource { .. }
                            | Call::GetCurrentResource { .. }
                            | Call::GetResource { .. }
                            | Call::GetNamedResource { .. }
                    ) {
                        self.memory
                            .write(GuestPtr(0x220), &self.process.mem_error.to_be_bytes())?;
                    }
                    if matches!(
                        call,
                        Call::Menu(
                            crate::menus::Op::Init
                                | crate::menus::Op::Load(_)
                                | crate::menus::Op::Bar(_)
                                | crate::menus::Op::AppendResources { .. }
                        ) | Call::AddResource(_)
                            | Call::CreateSpecResource(_)
                            | Call::CreateResourceFile(_)
                            | Call::ResourceFile(_)
                            | Call::FontInfo(_)
                            | Call::ResourceInfo(_)
                            | Call::Dialog(crate::dialogs::Op::Resource { .. })
                            | Call::CIcon(crate::cicon::Op::Get(_))
                            | Call::PixPat(crate::pixpat::Op::Get(_))
                            | Call::CountResources { .. }
                            | Call::GetIndexedResource { .. }
                            | Call::GetCurrentResource { .. }
                            | Call::GetResource { .. }
                            | Call::GetNamedResource { .. }
                            | Call::Device(crate::devices::Op::GetTable(_))
                            | Call::WriteResource(_)
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
                        Call::DrawOval { .. }
                        | Call::RoundRect { .. }
                        | Call::PortColor { .. }
                        | Call::RgbColor { .. }
                        | Call::GetRgbColor { .. }
                        | Call::NewPointer { .. }
                        | Call::PointerSize(_)
                        | Call::SetPointerSize { .. }
                        | Call::DisposePointer(_)
                        | Call::ResourceWindow(_)
                        | Call::NewControl(_)
                        | Call::TrackControl(_)
                        | Call::Control(_)
                        | Call::WindowInvalidation { .. }
                        | Call::WindowReference { .. }
                        | Call::WindowChange { .. }
                        | Call::NewWindow(_)
                        | Call::NewColorWindow(_)
                        | Call::Window { .. }
                        | Call::TextState { .. }
                        | Call::Rectangle(_)
                        | Call::ToolboxEvent { .. }
                        | Call::Gestalt(_)
                        | Call::SecondsToDate { .. }
                        | Call::ReadDateTime(_)
                        | Call::Decimal(_)
                        | Call::Button
                        | Call::StillDown
                        | Call::FindWindow { .. }
                        | Call::EmptyRect(_)
                        | Call::PointInRect { .. }
                        | Call::StandardGetFile(_)
                        | Call::StandardPutFile(_)
                        | Call::GetWMgrPort(_)
                        | Call::GetKeys(_)
                        | Call::GetMouse(_)
                        | Call::SystemTask
                        | Call::SysEnvirons { .. }
                        | Call::ColorPort { .. }
                        | Call::Color(_)
                        | Call::Device(_)
                        | Call::SetApplLimit(_)
                        | Call::MoreMasters
                        | Call::MaxApplZone
                        | Call::GetApplLimit
                        | Call::MoveBytes { .. }
                        | Call::Dialog(_)
                        | Call::Cursor(_)
                        | Call::MenuSelect(_)
                        | Call::HiliteMenu(_)
                        | Call::FixedMultiply { .. }
                        | Call::LongToFixed(_)
                        | Call::FixedRatio { .. }
                        | Call::Word { .. }
                        | Call::MenuChoice
                        | Call::Menu(_)
                        | Call::TextEdit(_)
                        | Call::PenLine { .. }
                        | Call::MoveTo { .. }
                        | Call::TextDraw(_)
                        | Call::TextWidth(_)
                        | Call::InitFonts
                        | Call::FontInfo(_)
                        | Call::Random
                        | Call::Logical { .. }
                        | Call::LoadFragment(_)
                        | Call::CloseFragment(_)
                        | Call::FragmentQuery(_)
                        | Call::InitGraf { .. }
                        | Call::Port { .. }
                        | Call::DrawRectangle { .. }
                        | Call::CopyMask(_)
                        | Call::CopyBits(_)
                        | Call::PlotCIcon { .. }
                        | Call::DrawPicture { .. }
                        | Call::SetOrigin { .. }
                        | Call::MovePortTo { .. }
                        | Call::PortSize { .. }
                        | Call::PenSize { .. }
                        | Call::PenMode(_)
                        | Call::PenNormal
                        | Call::PenVisibility(_)
                        | Call::WindowRepaint(_)
                        | Call::WindowVisibility(_)
                        | Call::RegionRecording(_)
                        | Call::Polygon(_)
                        | Call::Delay { .. }
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
                        | Call::HandleState { .. }
                        | Call::CopyHandle(_)
                        | Call::MoveHandleHigh(_)
                        | Call::HandleAttribute { .. }
                        | Call::DisposeHandle(_)
                        | Call::CombineRegions { .. }
                        | Call::Region { .. }
                        | Call::HandleSize(_)
                        | Call::SetHandleSize { .. }
                        | Call::ResourceInfo(_)
                        | Call::CIcon(_)
                        | Call::PixPat(_)
                        | Call::GWorld(_)
                        | Call::PixelState { .. }
                        | Call::PixelBase(_)
                        | Call::Pixels { .. }
                        | Call::CountResources { .. }
                        | Call::GetIndexedResource { .. }
                        | Call::GetCurrentResource { .. }
                        | Call::GetResource { .. }
                        | Call::SoundChannel(_)
                        | Call::SoundImmediate { .. }
                        | Call::SoundQueue { .. }
                        | Call::SoundDouble { .. }
                        | Call::SoundPauseFile(_)
                        | Call::SoundStartFile(_)
                        | Call::SoundPlay { .. }
                        | Call::SoundVersion
                        | Call::SoundStatus(_)
                        | Call::EqualString(_)
                        | Call::GetOutputVolume(_)
                        | Call::SetOutputVolume(_)
                        | Call::SoundVolume(_)
                        | Call::GetSoundVolume(_)
                        | Call::DeviceControl(_)
                        | Call::GetMmuMode
                        | Call::SwapMmuMode(_)
                        | Call::QuickDraw32Addressing
                        | Call::StripAddress(_)
                        | Call::Munger(_)
                        | Call::MemError
                        | Call::FreeMemory
                        | Call::MaxMemory(_)
                        | Call::ResError
                        | Call::WriteResource(_)
                        | Call::DetachResource(_)
                        | Call::ReleaseResource(_)
                        | Call::CurResFile
                        | Call::ResourceFile(_)
                        | Call::AddResource(_)
                        | Call::CreateSpecResource(_)
                        | Call::CreateResourceFile(_)
                        | Call::GetNamedResource { .. }
                        | Call::CreateDirectory(_)
                        | Call::FileInfo { .. }
                        | Call::FindFolder(_)
                        | Call::VolumeInfo { .. }
                        | Call::CatalogInfo(_)
                        | Call::FileIo { .. }
                        | Call::WorkingDirectory { .. }
                        | Call::Retrace(_)
                        | Call::RetraceHeader
                        | Call::IntegerToExtended { .. }
                        | Call::DivideExtended { .. }
                        | Call::SingleToExtended { .. }
                        | Call::DoubleToExtended { .. }
                        | Call::ExtendedToInteger { .. }
                        | Call::ExtendedToSingle { .. }
                        | Call::ExtendedToDouble { .. }
                        | Call::MultiplyExtended { .. }
                        | Call::SignExtended { .. }
                        | Call::RoundIntegralExtended { .. }
                        | Call::PowerExtended { .. }
                        | Call::NumericBinary { .. }
                        | Call::SquareRootExtended(_)
                        | Call::AddExtended { .. }
                        | Call::NumericCompare { .. }
                        | Call::NumericEnvironment { .. }
                        | Call::ClassifyExtended { .. }
                        | Call::CopyExtended { .. }
                        | Call::Elementary { .. }
                        | Call::AtanExtended(_)
                        | Call::Timer(_)
                        | Call::FlushVolume(_)
                        | Call::GetSpecInfo(_)
                        | Call::SetSpecInfo(_)
                        | Call::DeleteSpec(_)
                        | Call::MakeSpec(_)
                        | Call::NewAlias(_)
                        | Call::ResolveAlias(_)
                        | Call::ResolveAliasFile(_)
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
                                    foreground: u32::from(b[8]),
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
    fn poll_delay(&mut self, wait: DelayWait) -> Result<Option<Stop>> {
        if wait.remaining != 0 {
            let ticks = wait.remaining;
            self.delay_wait = Some(wait);
            return Ok(Some(Stop::Waiting { ticks }));
        }
        let now = self.tick_count()?;
        if let Some(p) = wait.final_ticks {
            self.memory.write(p, &now.to_be_bytes())?;
        }
        if let (Some(sr), Cpu::M68k(c)) = (wait.saved_sr, &mut self.cpu) {
            c.set_sr(sr);
        }
        wait.reply.apply(&mut self.cpu, &mut self.memory, now, 0)?;
        Ok(None)
    }
    fn poll_waits(&mut self) -> Result<Option<Stop>> {
        if let Some(wait) = self.delay_wait.take() {
            if let Some(stop) = self.poll_delay(wait)? {
                return Ok(Some(stop));
            }
        }
        if let Some(wait) = self.modal_wait.take() {
            if let Some(stop) = self.poll_modal(wait)? {
                return Ok(Some(stop));
            }
        }
        if let Some(wait) = self.standard_file_wait.take() {
            if let Some(stop) = self.poll_standard_file(wait)? {
                return Ok(Some(stop));
            }
        }
        if let Some(wait) = self.menu_wait.take() {
            if let Some(stop) = self.poll_menu_wait(wait)? {
                return Ok(Some(stop));
            }
        }
        if let Some(wait) = self.control_wait.take() {
            if let Some(stop) = self.poll_control_wait(wait)? {
                return Ok(Some(stop));
            }
        }
        if let Some(wait) = self.event_wait.take() {
            if let Some(stop) = self.poll_event_wait(wait)? {
                return Ok(Some(stop));
            }
        }

        if let Some(wait) = self.play_wait.take() {
            return self.poll_resource_play(wait);
        }
        if let Some(wait) = self.sound_wait.take() {
            return self.poll_sound_wait(wait);
        }
        Ok(None)
    }
    fn poll_resource_play(&mut self, mut wait: ResourcePlayWait) -> Result<Option<Stop>> {
        let mut sound = std::mem::take(&mut self.process.sound);
        let result = sound.poll_play(&mut self.memory, &mut self.process, &mut wait.play);
        let cleanup = if matches!(result,Ok(Some(e)) if e!=0) {
            sound.abandon_play(&mut self.memory, &mut self.process, &mut wait.play)
        } else {
            Ok(())
        };
        self.process.sound = sound;
        cleanup?;
        if let Some(error) = result? {
            wait.reply
                .apply(&mut self.cpu, &mut self.memory, error as i32 as u32, error)?;
            Ok(None)
        } else {
            self.play_wait = Some(wait);
            Ok(Some(Stop::Waiting { ticks: 1 }))
        }
    }
    fn poll_sound_wait(&mut self, wait: SoundWait) -> Result<Option<Stop>> {
        let mut sound = std::mem::take(&mut self.process.sound);
        let result = if wait.serial == 0 || sound.generation(wait.channel) != wait.serial {
            Ok(Some(-205))
        } else {
            sound.enqueue(
                &mut self.memory,
                &mut self.process,
                wait.channel,
                wait.command,
            )
        };
        self.process.sound = sound;
        if let Some(error) = result? {
            wait.reply
                .apply(&mut self.cpu, &mut self.memory, error as i32 as u32, error)?;
            Ok(None)
        } else {
            self.sound_wait = Some(wait);
            Ok(Some(Stop::Waiting { ticks: 1 }))
        }
    }
    fn suspend_waits(&mut self) -> Box<SuspendedWaits> {
        Box::new(SuspendedWaits {
            delay: self.delay_wait.take(),
            event: self.event_wait.take(),
            control: self.control_wait.take(),
            menu: self.menu_wait.take(),
            file: self.standard_file_wait.take(),
            modal: self.modal_wait.take(),
            sound: self.sound_wait.take(),
            play: self.play_wait.take(),
        })
    }
    fn restore_waits(&mut self, waits: SuspendedWaits) {
        self.delay_wait = waits.delay;
        self.event_wait = waits.event;
        self.control_wait = waits.control;
        self.menu_wait = waits.menu;
        self.standard_file_wait = waits.file;
        self.modal_wait = waits.modal;
        self.sound_wait = waits.sound;
        self.play_wait = waits.play;
    }
    fn enter_pending_retrace_callback(&mut self) -> Result<()> {
        if let Cpu::M68k(c) = &self.cpu {
            if c.get_sr() & 0x0700 != 0 {
                return Ok(());
            }
        }
        if self
            .continuations
            .iter()
            .any(|c| matches!(c.resume, Resume::Timer { .. } | Resume::Retrace { .. }))
        {
            return Ok(());
        }
        let Some(due) = self
            .process
            .retrace
            .pending(&mut self.memory, self.process.timers.now())?
        else {
            return Ok(());
        };
        if self.continuations.len() >= 32 {
            return Err(Error::Cpu("VBL callback depth exceeded".into()));
        }
        let routine = self.memory.u32(GuestPtr(due.task.0 + 6))?;
        let callee = crate::callback::retrace(&self.cpu, &mut self.memory, routine, due.task)?;
        self.process.retrace.begin(&mut self.memory, due)?;
        let waits = self.suspend_waits();
        let caller = std::mem::replace(&mut self.cpu, callee);
        self.continuations.push(Continuation {
            caller,
            resume: Resume::Retrace {
                task: due.task,
                waits,
            },
        });
        Ok(())
    }
    fn enter_pending_timer_callback(&mut self) -> Result<()> {
        if let Cpu::M68k(c) = &self.cpu {
            // The classic Time Manager uses the level-one VIA interrupt.
            if c.get_sr() & 0x0700 != 0 {
                return Ok(());
            }
        }
        // Timer callbacks are serialized; due tasks remain active until a safe
        // instruction boundary outside an earlier timer callback.
        if self
            .continuations
            .iter()
            .any(|c| matches!(c.resume, Resume::Timer { .. } | Resume::Retrace { .. }))
        {
            return Ok(());
        }
        while let Some(due) = self.process.timers.pending(&self.memory)? {
            if due.routine == 0 {
                self.process.timers.expire(&mut self.memory, due)?;
                continue;
            }
            if self.continuations.len() >= 32 {
                return Err(Error::Cpu("timer callback depth exceeded".into()));
            }
            let callee =
                crate::callback::timer(&self.cpu, &mut self.memory, due.routine, due.task)?;
            self.process.timers.expire(&mut self.memory, due)?;
            let waits = self.suspend_waits();
            let caller = std::mem::replace(&mut self.cpu, callee);
            self.continuations.push(Continuation {
                caller,
                resume: Resume::Timer { waits },
            });
            break;
        }
        Ok(())
    }
    fn enter_pending_sound_callback(&mut self) -> Result<()> {
        let sound = std::mem::take(&mut self.process.sound);
        let pending = sound.pending_callback(&self.memory, &mut self.process);
        self.process.sound = sound;
        let Some((channel, routine, argument)) = pending? else {
            return Ok(());
        };
        if self.continuations.len() >= 32 {
            return Err(Error::Cpu("sound callback depth exceeded".into()));
        }
        let callee =
            crate::callback::sound(&self.cpu, &mut self.memory, routine, channel, argument)?;
        let (serial, double_generation) = self.process.sound.begin_callback(channel);
        let waits = self.suspend_waits();
        let caller = std::mem::replace(&mut self.cpu, callee);
        self.continuations.push(Continuation {
            caller,
            resume: Resume::Sound {
                channel,
                serial,
                double_generation,
                waits,
            },
        });
        Ok(())
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
    fn finish_modal(&mut self, wait: ModalWait, hit: i16) -> Result<Option<Stop>> {
        wait.dialog
            .finish(&mut self.memory, &mut self.process, hit)?;
        wait.reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
        Ok(None)
    }
    fn modal_event(&mut self, wait: &mut ModalWait) -> Result<Option<i16>> {
        let ticks = self.tick_count()?;
        let gpu = self
            .graphics
            .as_ref()
            .ok_or_else(|| Error::Graphics("ModalDialog requires a GPU".into()))?;
        let mut manager = std::mem::take(&mut self.process.dialogs);
        let result = manager.modal_event(
            &mut self.memory,
            &mut self.process,
            gpu,
            &mut wait.dialog,
            ticks,
        );
        self.process.dialogs = manager;
        result
    }
    fn poll_modal(&mut self, mut wait: ModalWait) -> Result<Option<Stop>> {
        self.memory.validate_write(wait.dialog.destination, 2)?;
        if wait.dialog.tracking() {
            let gpu = self
                .graphics
                .as_ref()
                .ok_or_else(|| Error::Graphics("modal tracking requires GPU".into()))?;
            if let Some(hit) =
                wait.dialog
                    .poll_tracking(&mut self.memory, &mut self.process, gpu)?
            {
                return self.finish_modal(wait, hit);
            }
            if wait.dialog.tracking() {
                self.modal_wait = Some(wait);
                return Ok(Some(Stop::Waiting { ticks: 1 }));
            }
        }
        for _ in 0..20 {
            let ticks = self.tick_count()?;
            let found = self.receive_toolbox_event(0xff7f, wait.dialog.event(), true, ticks)?;
            if wait.dialog.filter != 0 && !wait.dialog.outside_click(&self.memory, &self.process)? {
                if self.continuations.len() >= 32 {
                    return Err(Error::Cpu("callback depth exceeded".into()));
                }
                self.memory.write(wait.dialog.hit(), &[0; 2])?;
                let (callee, result) = crate::callback::modal_filter(
                    &self.cpu,
                    &mut self.memory,
                    wait.dialog.filter,
                    [
                        wait.dialog.dialog,
                        wait.dialog.event().0,
                        wait.dialog.hit().0,
                    ],
                )?;
                let caller = std::mem::replace(&mut self.cpu, callee);
                self.continuations.push(Continuation {
                    caller,
                    resume: Resume::Modal {
                        wait: Box::new(wait),
                        result,
                    },
                });
                return Ok(None);
            }
            if let Some(hit) = self.modal_event(&mut wait)? {
                return self.finish_modal(wait, hit);
            }
            if !found || wait.dialog.tracking() {
                break;
            }
        }
        self.modal_wait = Some(wait);
        Ok(Some(Stop::Waiting { ticks: 1 }))
    }
    fn poll_standard_file(&mut self, mut wait: StandardFileWait) -> Result<Option<Stop>> {
        let ticks = self.tick_count()?;
        let gpu = self
            .graphics
            .as_ref()
            .ok_or_else(|| Error::Graphics("Standard File requires a GPU".into()))?;
        for _ in 0..20 {
            if let Some((routine, argument)) =
                wait.dialog
                    .filter_call(&mut self.memory, &mut self.process, gpu, ticks)?
            {
                if self.continuations.len() >= 32 {
                    return Err(Error::Cpu("callback depth exceeded".into()));
                }
                let (callee, result) =
                    crate::callback::file_filter(&self.cpu, &mut self.memory, routine, argument)?;
                let caller = std::mem::replace(&mut self.cpu, callee);
                self.continuations.push(Continuation {
                    caller,
                    resume: Resume::StandardFile {
                        wait: Box::new(wait),
                        result,
                    },
                });
                return Ok(None);
            }
            let event = crate::events::prepare_remove(
                &self.memory,
                (1 << 1) | (1 << 2) | (1 << 3) | (1 << 5),
            )?;
            let record = event.as_ref().map(|e| e.record);
            let done = wait
                .dialog
                .poll(&mut self.memory, &mut self.process, gpu, record, ticks)?;
            if let Some(event) = event {
                event.commit(&mut self.memory)?;
            }
            if let Some(good) = done {
                if wait
                    .dialog
                    .finish(&mut self.memory, &mut self.process, gpu, good)?
                {
                    wait.reply.apply(&mut self.cpu, &mut self.memory, 0, 0)?;
                    return Ok(None);
                }
            }
            if record.is_none() {
                break;
            }
        }
        self.standard_file_wait = Some(wait);
        Ok(Some(Stop::Waiting { ticks: 1 }))
    }
    fn poll_menu_wait(&mut self, mut wait: MenuWait) -> Result<Option<Stop>> {
        if let crate::abi::Reply::StackLong(p) = wait.reply {
            self.memory.validate_write(p, 4)?;
        }
        let release = crate::events::prepare_remove(&self.memory, 1 << 2)?;
        let released = release.is_some() || self.process.input.modifiers & 0x80 != 0;
        let bytes = if let Some(e) = &release {
            e.record[10..14].to_vec()
        } else {
            self.memory.read(crate::events::MOUSE, 4)?.to_vec()
        };
        let point = [
            i16::from_be_bytes(bytes[..2].try_into().unwrap()),
            i16::from_be_bytes(bytes[2..].try_into().unwrap()),
        ];
        let gpu = self
            .graphics
            .as_ref()
            .ok_or_else(|| Error::Graphics("menu tracking requires a GPU".into()))?;
        let manager = std::mem::take(&mut self.process.menus);
        let result = manager.poll_tracking(
            &mut self.memory,
            &mut self.process,
            gpu,
            &mut wait.tracking,
            point,
            released,
        );
        self.process.menus = manager;
        if let Some(value) = result? {
            if let Some(e) = release {
                e.commit(&mut self.memory)?;
            }
            wait.reply
                .apply(&mut self.cpu, &mut self.memory, value, 0)?;
            Ok(None)
        } else {
            self.menu_wait = Some(wait);
            Ok(Some(Stop::Waiting { ticks: 1 }))
        }
    }
    fn poll_control_wait(&mut self, wait: ControlWait) -> Result<Option<Stop>> {
        if let crate::abi::Reply::StackWord(p) = wait.reply {
            self.memory.validate_write(p, 2)?;
        }
        let release = crate::events::prepare_remove(&self.memory, 1 << 2)?;
        let released = release.is_some() || self.process.input.modifiers & 0x80 != 0;
        let point = if let Some(event) = &release {
            event.record[10..14].try_into().unwrap()
        } else {
            self.memory
                .read(crate::events::MOUSE, 4)?
                .try_into()
                .unwrap()
        };
        let mut controls = std::mem::take(&mut self.process.controls);
        let result = controls.poll_tracking(
            &mut self.memory,
            &mut self.process,
            self.graphics.as_ref(),
            &wait.tracking,
            point,
            released,
        );
        self.process.controls = controls;
        if let Some(part) = result? {
            if let Some(event) = release {
                event.commit(&mut self.memory)?;
            }
            wait.reply
                .apply(&mut self.cpu, &mut self.memory, u32::from(part), 0)?;
            Ok(None)
        } else {
            self.control_wait = Some(wait);
            Ok(Some(Stop::Waiting { ticks: 1 }))
        }
    }
    fn mouse_region_input(&mut self, region: GuestPtr) -> Result<Option<crate::events::Input>> {
        if region.0 == 0 {
            return Ok(None);
        }
        let old_error = self.process.mem_error;
        let bytes = crate::regions::read(&self.memory, &mut self.process, region.0);
        self.process.mem_error = old_error;
        let spans = crate::region_geometry::decode(&bytes?)?;
        if spans.is_empty() {
            return Ok(None);
        }
        let point = self.memory.read(crate::events::MOUSE, 4)?;
        let v = i16::from_be_bytes(point[..2].try_into().unwrap());
        let h = i16::from_be_bytes(point[2..].try_into().unwrap());
        if spans.iter().any(|r| {
            i32::from(h) >= r.left
                && i32::from(h) < r.right
                && i32::from(v) >= r.top
                && i32::from(v) < r.bottom
        }) {
            return Ok(None);
        }
        Ok(Some(crate::events::Input {
            vertical: v,
            horizontal: h,
            ..self.process.input
        }))
    }
    fn poll_event_wait(&mut self, wait: EventWait) -> Result<Option<Stop>> {
        let movement = self
            .mouse_region_input(wait.region)?
            .filter(|_| wait.mask & 0x8000 != 0);
        let ready = self.toolbox_available(wait.mask)?;
        if !ready && movement.is_none() && wait.remaining != 0 {
            let ticks = wait.remaining;
            self.event_wait = Some(wait);
            return Ok(Some(Stop::Waiting { ticks }));
        }
        // Validate deferred ABI output before removing an event.
        if let crate::abi::Reply::StackByte(p) = wait.reply {
            self.memory.validate_write(p, 1)?;
        }
        let now = self.tick_count()?;
        let found = match (ready, movement) {
            (false, Some(input)) => {
                self.memory.write(
                    wait.destination,
                    &crate::events::record(15, 0xfa00_0000, now, input),
                )?;
                true
            }
            _ => self.receive_toolbox_event(wait.mask, wait.destination, true, now)?,
        };
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
            Resume::Retrace { task, waits } => {
                self.process.retrace.finish(&mut self.memory, task)?;
                self.services.completed_vbl_callbacks = self
                    .services
                    .completed_vbl_callbacks
                    .checked_add(1)
                    .ok_or_else(|| Error::Cpu("VBL callback count overflow".into()))?;
                self.restore_waits(*waits);
            }
            Resume::Timer { waits } => {
                self.services.completed_timer_callbacks = self
                    .services
                    .completed_timer_callbacks
                    .checked_add(1)
                    .ok_or_else(|| Error::Cpu("timer callback diagnostic overflow".into()))?;
                self.restore_waits(*waits);
            }
            Resume::Sound {
                channel,
                serial,
                double_generation,
                waits,
            } => {
                let mut sound = std::mem::take(&mut self.process.sound);
                let result = sound.finish_callback(
                    &mut self.memory,
                    &mut self.process,
                    channel,
                    serial,
                    double_generation,
                );
                self.process.sound = sound;
                result?;
                self.services.completed_sound_callbacks = self
                    .services
                    .completed_sound_callbacks
                    .checked_add(1)
                    .ok_or_else(|| Error::Cpu("sound callback diagnostic count overflow".into()))?;
                self.restore_waits(*waits);
            }
            Resume::Modal { mut wait, result } => {
                if result.read(&self.memory, value)? {
                    let hit = self.memory.u16(wait.dialog.hit())? as i16;
                    return self.finish_modal(*wait, hit);
                }
                if let Some(hit) = self.modal_event(&mut wait)? {
                    return self.finish_modal(*wait, hit);
                }
                self.modal_wait = Some(*wait);
                return Ok(Some(Stop::Waiting { ticks: 1 }));
            }
            Resume::StandardFile { mut wait, result } => {
                wait.dialog.filtered(result.read(&self.memory, value)?)?;
                return self.poll_standard_file(*wait);
            }
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
