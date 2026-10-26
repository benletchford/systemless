//! Cooperative classic QuickDraw getPicProc calls.

use super::dispatch::TrapDispatcher;
use super::pict::{picture_stream_progress, PictureStreamProgress};
use crate::cpu::{CpuOps, Register};
use crate::guest_call::GuestCallTarget;
use crate::guest_procedure::GuestIsa;
use crate::memory::{MacMemoryBus, MemoryBus};

const MAX_STREAM: usize = 32 * 1024 * 1024;
const MAX_READ: usize = i16::MAX as usize;

#[derive(Clone, Debug)]
pub(crate) struct PictureStreamFrame {
    handle: u32,
    rect: u32,
    caller_sp: u32,
    caller_pc: u32,
    callback: u32,
    buffer: u32,
    return_gate: u32,
    requested: usize,
    bytes: Vec<u8>,
    ready: bool,
}

pub(super) enum PictureStreamInput {
    Resident,
    Deferred,
    Materialized(Vec<u8>),
}

impl TrapDispatcher {
    pub(super) fn prepare_picture_stream<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        handle: u32,
        rect: u32,
    ) -> PictureStreamInput {
        let sp = cpu.read_reg(Register::A7);
        let pc = cpu.read_reg(Register::PC);
        if self.picture_stream_stack.last().is_some_and(|frame| {
            frame.ready
                && frame.handle == handle
                && frame.rect == rect
                && frame.caller_sp == sp + 8
                && frame.caller_pc == pc
        }) {
            return self.take_picture_stream(bus);
        }
        let a5 = cpu.read_reg(Register::A5);
        if !bus.is_guest_address_mapped(a5, 4) {
            return PictureStreamInput::Resident;
        }
        let globals = bus.read_long(a5);
        if !bus.is_guest_address_mapped(globals, 4) {
            return PictureStreamInput::Resident;
        }
        let port = bus.read_long(globals);
        if handle == 0
            || !bus.is_guest_address_mapped(port, 108)
            || !bus.is_guest_address_mapped(handle, 4)
        {
            return PictureStreamInput::Resident;
        }
        let procs = bus.read_long(port + 104);
        if procs == 0 || !bus.is_guest_address_mapped(procs, 48) {
            return PictureStreamInput::Resident;
        }
        let callback = bus.read_long(procs + 44);
        if callback == 0 || bus.system_trap_gateway(0xa8ee) == Some(callback) {
            return PictureStreamInput::Resident;
        }
        let data = bus.read_long(handle);
        if !bus.is_guest_address_mapped(data, 10) {
            return PictureStreamInput::Materialized(Vec::new());
        }
        let buffer = bus.alloc(MAX_READ as u32);
        let return_gate = bus.alloc(2);
        if buffer == 0 || return_gate == 0 {
            if buffer != 0 {
                bus.free(buffer);
            }
            if return_gate != 0 {
                bus.free(return_gate);
            }
            return PictureStreamInput::Materialized(Vec::new());
        }
        // Only the registered PC/SP pair can resume this internal DrawPicture.
        // It is consumed before public trap patch routing.
        bus.write_word(return_gate, 0xa8f6);
        self.picture_stream_stack.push(PictureStreamFrame {
            handle,
            rect,
            caller_sp: sp + 8,
            caller_pc: pc,
            callback,
            buffer,
            return_gate,
            requested: 0,
            bytes: bus.read_bytes(data, 10),
            ready: false,
        });
        if self.advance_picture_stream(cpu, bus) {
            PictureStreamInput::Deferred
        } else {
            self.take_picture_stream(bus)
        }
    }

    fn take_picture_stream(&mut self, bus: &mut MacMemoryBus) -> PictureStreamInput {
        let frame = self
            .picture_stream_stack
            .pop()
            .expect("ready picture stream");
        bus.free(frame.buffer);
        bus.free(frame.return_gate);
        PictureStreamInput::Materialized(frame.bytes)
    }

    /// Schedule one Pascal QDGetPicProc(dataPtr, byteCount). The procedure
    /// receives the last argument nearest the return address and removes its
    /// six argument bytes on return; native CPU execution owns every step.
    fn advance_picture_stream<C: CpuOps>(&mut self, cpu: &mut C, bus: &mut MacMemoryBus) -> bool {
        let frame = self
            .picture_stream_stack
            .last_mut()
            .expect("active picture stream");
        let required = match picture_stream_progress(&frame.bytes) {
            PictureStreamProgress::Complete(end) => {
                frame.bytes.truncate(end);
                frame.ready = true;
                return false;
            }
            PictureStreamProgress::Need(required) if required <= MAX_STREAM => required,
            _ => {
                frame.bytes.clear();
                frame.ready = true;
                return false;
            }
        };
        frame.requested = required
            .saturating_sub(frame.bytes.len())
            .max(1)
            .min(MAX_READ);
        let Some(sp) = frame.caller_sp.checked_sub(10) else {
            frame.bytes.clear();
            frame.ready = true;
            return false;
        };
        if !bus.is_guest_address_writable(sp, 10)
            || !self.guest_calls.begin_m68k_with_operation(
                GuestCallTarget {
                    isa: GuestIsa::M68k,
                    entry: frame.callback,
                    rtoc: 0,
                },
                frame.return_gate,
                frame.caller_sp,
                None,
                None,
            )
        {
            frame.bytes.clear();
            frame.ready = true;
            return false;
        }
        bus.write_long(sp, frame.return_gate);
        bus.write_word(sp + 4, frame.requested as u16);
        bus.write_long(sp + 6, frame.buffer);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, frame.callback);
        true
    }

    pub(crate) fn resume_picture_stream<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<crate::Result<()>> {
        let frame = self.picture_stream_stack.last_mut()?;
        if frame.ready
            || cpu.read_reg(Register::PC) != frame.return_gate + 2
            || cpu.read_reg(Register::A7) != frame.caller_sp
        {
            return None;
        }
        frame
            .bytes
            .extend(bus.read_bytes(frame.buffer, frame.requested));
        if self.advance_picture_stream(cpu, bus) {
            return Some(Ok(()));
        }
        let frame = self.picture_stream_stack.last().unwrap();
        // Restore the original invocation for the internal renderer. The
        // ready frame supplies bytes without invoking getPicProc again; the
        // original handle and its ten-byte header remain untouched.
        let sp = frame.caller_sp - 8;
        bus.write_long(sp, frame.rect);
        bus.write_long(sp + 4, frame.handle);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, frame.caller_pc);
        self.dispatch_quickdraw(true, 0x0f6, cpu, bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn classic_drawpicture_streams_native_getpicproc_and_preserves_header() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        let caller_pc = 0x340002;
        cpu.write_reg(Register::PC, caller_pc);
        let cursor = 0x350100;
        let handle = 0x350200;
        let header = 0x350210;
        let rect = 0x350230;
        let procs = 0x350300;
        let callback = 0x360000;
        let stream = 0x370000;
        let picture_header = [0, 0, 0, 0, 0, 0, 0, 1, 0, 1];
        let mut definition = vec![0x11, 1];
        for count in [40_000u16, 30_000] {
            definition.extend_from_slice(&[0xa1, 0, 0]);
            definition.extend_from_slice(&count.to_be_bytes());
            definition.extend(std::iter::repeat_n(0xff, count as usize));
        }
        definition.extend_from_slice(&[0x31, 0, 0, 0, 0, 0, 1, 0, 1, 0xff]);
        bus.write_bytes(header, &picture_header);
        bus.write_bytes(stream, &definition);
        bus.write_long(cursor, stream);
        bus.write_long(handle, header);
        bus.write_bytes(rect, &picture_header[2..]);
        bus.write_long(0x181000 + 104, procs);
        bus.write_long(procs + 44, callback);
        // Real Pascal procedure: copy exactly byteCount bytes from its
        // persistent source cursor, then pop six arguments and return.
        let words = [
            0x302f, 4, // move.w 4(sp),d0
            0x206f, 6, // movea.l 6(sp),a0
            0x2279, 0x0035, 0x0100, // movea.l cursor,a1
            0x5340, // subq.w #1,d0
            0x10d9, // move.b (a1)+,(a0)+
            0x51c8, 0xfffc, // dbra d0,copy
            0x23c9, 0x0035, 0x0100, // move.l a1,cursor
            0x225f, // movea.l (sp)+,a1
            0x5c8f, // addq.l #6,sp
            0x4ed1, // jmp (a1)
        ];
        for (index, word) in words.into_iter().enumerate() {
            bus.write_word(callback + index as u32 * 2, word);
        }
        let screen = bus.read_long(0x181000 + 2);
        bus.write_byte(screen, 0);
        bus.write_long(TEST_SP, rect);
        bus.write_long(TEST_SP + 4, handle);
        bus.write_long(TEST_SP + 8, 0xdead_beef);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::PC), callback);
        assert_eq!(
            bus.read_long(cursor),
            stream,
            "callback must execute through the CPU"
        );
        assert_eq!(bus.read_byte(screen), 0);
        assert!(dispatcher
            .resume_picture_stream(&mut cpu, &mut bus)
            .is_none());
        let mut finished = false;
        for _ in 0..1_000_000 {
            if cpu.read_reg(Register::PC) == caller_pc && dispatcher.picture_stream_stack.is_empty()
            {
                finished = true;
                break;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected callback CPU result"),
            }
        }
        assert!(
            finished,
            "callback did not finish within the instruction bound"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), 0xdead_beef);
        assert_eq!(bus.read_long(cursor), stream + definition.len() as u32);
        assert_eq!(bus.read_bytes(header, 10), picture_header);
        assert_eq!(bus.read_byte(screen), 0x80);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
    }
}
