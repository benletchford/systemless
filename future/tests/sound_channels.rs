mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    sound::{self, NewChannel, Op},
};
const STACK: u32 = STACK_BASE + 0x8000;
const OUT: GuestPtr = GuestPtr(0x50000000);
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"SndNewChannel", b"SndDisposeChannel"], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(OUT, vec![0xa5; 4096], Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa8, 7, 0xa8, 1],
            Permissions::RX,
        )
        .unwrap();
    app
}
fn frame(op: Op) -> Vec<u8> {
    let mut bytes = Vec::new();
    match op {
        Op::New(r) => {
            bytes.extend(r.callback.to_be_bytes());
            bytes.extend(r.init.to_be_bytes());
            bytes.extend(r.synth.to_be_bytes());
            bytes.extend(r.destination.0.to_be_bytes());
        }
        Op::Dispose { channel, quiet_now } => {
            bytes.extend([u8::from(quiet_now), 0x5a]);
            bytes.extend(channel.to_be_bytes());
        }
    }
    bytes.extend([0xa5, 0xa5]);
    bytes
}
fn invoke(a: &mut LoadedApplication, arch: Architecture, op: Op) -> i16 {
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &0xbeefu16.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &0xcafeu16.to_be_bytes())
        .unwrap();
    let size = match op {
        Op::New(_) => 14,
        Op::Dispose { .. } => 6,
    };
    match arch {
        Architecture::M68k => {
            a.runtime.memory.write(GuestPtr(STACK), &frame(op)).unwrap();
            let pc = 0x51000000 + if matches!(op, Op::New(_)) { 0 } else { 2 };
            a.runtime.cpu = Cpu::new(arch, pc, STACK);
            if let Cpu::M68k(c) = &mut a.runtime.cpu {
                c.dar[0] = 0x12345678;
            }
        }
        Architecture::PowerPc => {
            let index = if matches!(op, Op::New(_)) { 0 } else { 1 };
            let data = a.info.sections[1].0;
            let vector = a
                .runtime
                .memory
                .u32(GuestPtr(data + 8 + index * 4))
                .unwrap();
            let gate = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
            a.runtime.cpu = Cpu::new(arch, gate, STACK);
            if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                c.gpr[9] = 0x12345678;
                match op {
                    Op::New(r) => {
                        c.gpr[3] = r.destination.0;
                        c.gpr[4] = r.synth as i32 as u32;
                        c.gpr[5] = r.init;
                        c.gpr[6] = r.callback;
                    }
                    Op::Dispose { channel, quiet_now } => {
                        c.gpr[3] = channel;
                        c.gpr[4] = u32::from(quiet_now);
                    }
                }
            }
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0xbeef);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0xcafe);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(c.dar[15], STACK + size);
            a.runtime.memory.u16(GuestPtr(STACK + size)).unwrap() as i16
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[9], 0x12345678);
            let result = c.gpr[3] as i16;
            assert_eq!(c.gpr[3], result as i32 as u32);
            result
        }
    }
}
fn new(destination: GuestPtr) -> Op {
    Op::New(NewChannel {
        destination,
        synth: 5,
        init: 0x80,
        callback: 0x12345678,
    })
}
#[test]
fn native_and_ppc_frames_validate_all_outputs_before_consuming_arguments() {
    let mut m = Memory::default();
    m.map(OUT, vec![0; 4], Permissions::RW).unwrap();
    for op in [
        new(OUT),
        Op::New(NewChannel {
            destination: OUT,
            synth: -2,
            init: 0x10203040,
            callback: 0xfedcba98,
        }),
        Op::Dispose {
            channel: 0x10203040,
            quiet_now: true,
        },
        Op::Dispose {
            channel: 0,
            quiet_now: false,
        },
    ] {
        let bytes = frame(op);
        let size = bytes.len() - 2;
        let mut mem = Memory::default();
        mem.map(OUT, vec![0; 4], Permissions::RW).unwrap();
        mem.map(GuestPtr(0x1000), bytes.clone(), Permissions::RW)
            .unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        let trap = if matches!(op, Op::New(_)) {
            0xa807
        } else {
            0xa801
        };
        assert_eq!(
            Abi::trap(trap, c, &mem).unwrap(),
            (
                Call::SoundChannel(op),
                Reply::StackWord(GuestPtr(0x1000 + size as u32))
            )
        );
        assert_eq!(c.dar[15], 0x1000 + size as u32);
        mem.map(
            GuestPtr(0x2000),
            bytes[..bytes.len() - 1].to_vec(),
            Permissions::RW,
        )
        .unwrap();
        c.dar[15] = 0x2000;
        assert!(Abi::trap(trap, c, &mem).is_err());
        assert_eq!(c.dar[15], 0x2000);
        mem.map(GuestPtr(0x3000), bytes, Permissions::RX).unwrap();
        c.dar[15] = 0x3000;
        assert!(Abi::trap(trap, c, &mem).is_err());
        assert_eq!(c.dar[15], 0x3000);
    }
    assert_eq!(
        Abi::resolve(b"InterfaceLib", b"SndNewChannel"),
        Some(Service::SndNewChannel)
    );
    assert_eq!(
        Abi::resolve(b"InterfaceLib", b"SndDisposeChannel"),
        Some(Service::SndDisposeChannel)
    );
    assert_ne!(
        systemless_future::traps::address(0xa807, true),
        systemless_future::traps::UNIMPLEMENTED
    );
    assert_ne!(
        systemless_future::traps::address(0xa801, true),
        systemless_future::traps::UNIMPLEMENTED
    );
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, STACK);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x50000001;
    c.gpr[4] = 5;
    assert!(Abi::import(Service::SndNewChannel, c, &m).is_err());
    c.gpr[3] = OUT.0;
    c.gpr[4] = 0xabcdfffe;
    c.gpr[5] = 0x10203040;
    c.gpr[6] = 0xfedcba98;
    assert_eq!(
        Abi::import(Service::SndNewChannel, c, &m).unwrap(),
        (
            Call::SoundChannel(Op::New(NewChannel {
                destination: OUT,
                synth: -2,
                init: 0x10203040,
                callback: 0xfedcba98
            })),
            Reply::PpcR3
        )
    );
    c.gpr[3] = 0x10203040;
    c.gpr[4] = 0x12340100;
    assert_eq!(
        Abi::import(Service::SndDisposeChannel, c, &m).unwrap(),
        (
            Call::SoundChannel(Op::Dispose {
                channel: 0x10203040,
                quiet_now: false
            }),
            Reply::PpcR3
        )
    );
}
#[test]
fn both_backends_share_live_channel_list_and_owned_disposal_frees_only_its_allocations() {
    for creator in [Architecture::M68k, Architecture::PowerPc] {
        let disposer = if creator == Architecture::M68k {
            Architecture::PowerPc
        } else {
            Architecture::M68k
        };
        let mut a = app();
        let resource = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 8, true)
            .unwrap();
        let mut channels = Vec::new();
        for _ in 0..3 {
            a.runtime.memory.write(OUT, &0u32.to_be_bytes()).unwrap();
            assert_eq!(invoke(&mut a, creator, new(OUT)), 0);
            let channel = a.runtime.memory.u32(OUT).unwrap();
            let queue = a.runtime.memory.u32(GuestPtr(channel + 4)).unwrap();
            assert_eq!(
                a.runtime.process.sound.configuration(channel),
                Some((5, 0x80))
            );
            assert_eq!(a.runtime.process.pointer_size(channel), sound::CHANNEL_SIZE);
            assert_eq!(a.runtime.process.pointer_size(queue), 1024);
            assert_eq!(
                a.runtime.memory.u32(GuestPtr(channel + 8)).unwrap(),
                0x12345678
            );
            assert_eq!(a.runtime.memory.u16(GuestPtr(channel + 30)).unwrap(), 128);
            assert_eq!(
                a.runtime.memory.u32(GuestPtr(channel)).unwrap(),
                channels.last().map_or(0, |&(c, _)| c)
            );
            channels.push((channel, queue));
        }
        let (middle, queue) = channels[1];
        a.runtime
            .memory
            .write(GuestPtr(middle + 12), &resource.to_be_bytes())
            .unwrap();
        assert_eq!(
            invoke(
                &mut a,
                disposer,
                Op::Dispose {
                    channel: middle,
                    quiet_now: false
                }
            ),
            0
        );
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(channels[2].0)).unwrap(),
            channels[0].0
        );
        assert_eq!(a.runtime.process.sound.configuration(middle), None);
        assert_eq!(a.runtime.process.pointer_size(middle) as i32, -111);
        assert_eq!(a.runtime.process.pointer_size(queue) as i32, -111);
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, resource)
                .unwrap(),
            8
        );
        assert_eq!(
            invoke(
                &mut a,
                creator,
                Op::Dispose {
                    channel: middle,
                    quiet_now: true
                }
            ),
            -205
        );
        for index in [2, 0] {
            assert_eq!(
                invoke(
                    &mut a,
                    disposer,
                    Op::Dispose {
                        channel: channels[index].0,
                        quiet_now: true
                    }
                ),
                0
            );
        }
        assert_eq!(
            invoke(
                &mut a,
                creator,
                Op::Dispose {
                    channel: 0,
                    quiet_now: false
                }
            ),
            -205
        );
        assert_eq!(
            invoke(
                &mut a,
                creator,
                Op::Dispose {
                    channel: u32::MAX,
                    quiet_now: true
                }
            ),
            -205
        );
    }
}
#[test]
fn caller_records_keep_user_data_guards_and_storage_and_unsupported_requests_do_not_allocate() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let channel = OUT.0 + 128;
        a.runtime
            .memory
            .write(
                GuestPtr(channel),
                &vec![0x5a; sound::CHANNEL_SIZE as usize + 4],
            )
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(channel + 12), &0xfeed1234u32.to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(channel + 30), &2u16.to_be_bytes())
            .unwrap();
        a.runtime.memory.write(OUT, &channel.to_be_bytes()).unwrap();
        assert_eq!(invoke(&mut a, arch, new(OUT)), 0);
        let queue = a.runtime.memory.u32(GuestPtr(channel + 4)).unwrap();
        assert_eq!(a.runtime.process.pointer_size(queue), 16);
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(channel + 12)).unwrap(),
            0xfeed1234
        );
        let bytes = a
            .runtime
            .memory
            .read(GuestPtr(channel), sound::CHANNEL_SIZE as usize + 4)
            .unwrap()
            .to_vec();
        assert_eq!(invoke(&mut a, arch, new(OUT)), -205);
        assert_eq!(
            a.runtime
                .memory
                .read(GuestPtr(channel), bytes.len())
                .unwrap(),
            bytes
        );
        assert_eq!(
            invoke(
                &mut a,
                arch,
                Op::Dispose {
                    channel,
                    quiet_now: true
                }
            ),
            0
        );
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(channel + 12)).unwrap(),
            0xfeed1234
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(channel + 36), 1028).unwrap(),
            [0x5a; 1028]
        );
        assert_eq!(a.runtime.process.pointer_size(queue) as i32, -111);
        for (synth, init, error) in [
            (2, 0, -204),
            (1, 0, -201),
            (3, 0, -201),
            (5, 0x300, -201),
            (5, 0x40, -201),
        ] {
            let op = Op::New(NewChannel {
                destination: OUT,
                synth,
                init,
                callback: 0,
            });
            let before = a.runtime.memory.read(OUT, 4096).unwrap().to_vec();
            assert_eq!(invoke(&mut a, arch, op), error);
            assert_eq!(a.runtime.memory.read(OUT, 4096).unwrap(), before);
        }
    }
}

#[test]
fn allocation_failure_rolls_back_internal_queue_and_corrupt_records_cannot_free_live_storage() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let large = a
            .runtime
            .process
            .new_pointer(
                &mut a.runtime.memory,
                systemless_future::process::HEAP_SIZE - 1280,
                true,
            )
            .unwrap();
        assert_ne!(large, 0);
        a.runtime.memory.write(OUT, &0u32.to_be_bytes()).unwrap();
        assert_eq!(invoke(&mut a, arch, new(OUT)), -108);
        assert_eq!(a.runtime.memory.u32(OUT).unwrap(), 0);
        let remainder = a
            .runtime
            .process
            .new_pointer(&mut a.runtime.memory, 1280, true)
            .unwrap();
        assert_ne!(remainder, 0);
        a.runtime.process.dispose_pointer(remainder);
        a.runtime.process.dispose_pointer(large);
        assert_eq!(invoke(&mut a, arch, new(OUT)), 0);
        let channel = a.runtime.memory.u32(OUT).unwrap();
        let before = a
            .runtime
            .memory
            .read(GuestPtr(channel), sound::CHANNEL_SIZE as usize)
            .unwrap()
            .to_vec();
        for (offset, data) in [
            (4, vec![0, 0, 0, 0]),
            (16, vec![0, 0, 0, 1]),
            (30, vec![0, 1]),
            (32, vec![0, 1]),
        ] {
            a.runtime
                .memory
                .write(GuestPtr(channel + offset), &data)
                .unwrap();
            assert_eq!(
                invoke(
                    &mut a,
                    arch,
                    Op::Dispose {
                        channel,
                        quiet_now: true
                    }
                ),
                -205
            );
            assert_eq!(a.runtime.process.pointer_size(channel), sound::CHANNEL_SIZE);
            a.runtime.memory.write(GuestPtr(channel), &before).unwrap();
        }
        assert_eq!(
            invoke(
                &mut a,
                arch,
                Op::Dispose {
                    channel,
                    quiet_now: false
                }
            ),
            0
        );
    }
}
#[test]
fn channel_capacity_is_explicit_and_disposal_releases_a_slot() {
    let mut a = app();
    let mut first = 0;
    for index in 0..64 {
        a.runtime.memory.write(OUT, &0u32.to_be_bytes()).unwrap();
        assert_eq!(invoke(&mut a, Architecture::PowerPc, new(OUT)), 0);
        if index == 0 {
            first = a.runtime.memory.u32(OUT).unwrap();
        }
    }
    a.runtime.memory.write(OUT, &0u32.to_be_bytes()).unwrap();
    assert_eq!(invoke(&mut a, Architecture::M68k, new(OUT)), -201);
    assert_eq!(a.runtime.memory.u32(OUT).unwrap(), 0);
    assert_eq!(
        invoke(
            &mut a,
            Architecture::M68k,
            Op::Dispose {
                channel: first,
                quiet_now: false
            }
        ),
        0
    );
    assert_eq!(invoke(&mut a, Architecture::PowerPc, new(OUT)), 0);
}
