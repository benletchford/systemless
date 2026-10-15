mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::Call,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory, Permissions},
    polygons::{Op, Verb},
    process::Process,
    quickdraw::Ports,
};
fn setup() -> (Memory, Process, Ports) {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0; 32], Permissions::RW)
        .unwrap();
    m.write(
        GuestPtr(0x1000),
        &[0, 0, 0x20, 0, 0, 2, 0, 0, 0, 0, 0, 16, 0, 16],
    )
    .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    let mut p = Process::default();
    ports.open(&mut m, &mut p, GuestPtr(0x1080)).unwrap();
    p.ports = Some(ports);
    (m, p, ports)
}
fn op(
    m: &mut Memory,
    p: &mut Process,
    gpu: Option<&Graphics>,
    op: Op,
) -> systemless_future::Result<u32> {
    let mut manager = std::mem::take(&mut p.polygons);
    let r = manager.call(m, p, gpu, op);
    p.polygons = manager;
    r
}
fn position(m: &mut Memory, h: i16, v: i16) {
    let mut b = Vec::new();
    b.extend(v.to_be_bytes());
    b.extend(h.to_be_bytes());
    m.write(GuestPtr(0x1080 + 48), &b).unwrap();
}
fn line(
    m: &mut Memory,
    p: &mut Process,
    gpu: Option<&Graphics>,
    h: i16,
    v: i16,
) -> systemless_future::Result<bool> {
    let mut manager = std::mem::take(&mut p.polygons);
    let r = manager.line(m, p, gpu, h, v, false);
    p.polygons = manager;
    r
}
fn bytes(m: &Memory, p: &mut Process, h: u32) -> Vec<u8> {
    let n = p.handle_size(m, h).unwrap();
    m.read(GuestPtr(m.u32(GuestPtr(h)).unwrap()), n as usize)
        .unwrap()
        .to_vec()
}
fn record(m: &mut Memory, p: &mut Process, points: &[(i16, i16)]) -> u32 {
    position(m, points[0].0, points[0].1);
    let h = op(m, p, None, Op::Open).unwrap();
    for &(x, y) in &points[1..] {
        assert!(line(m, p, None, x, y).unwrap());
    }
    op(m, p, None, Op::Close).unwrap();
    h
}
#[test]
fn polygon_recording_uses_live_handles_and_balances_visibility_without_screen_writes() {
    let (mut m, mut p, ports) = setup();
    m.write(GuestPtr(0x2000), &[0x5a; 32]).unwrap();
    position(&mut m, 2, 2);
    m.write(GuestPtr(0x1080 + 66), &(-2i16).to_be_bytes())
        .unwrap();
    let h = op(&mut m, &mut p, None, Op::Open).unwrap();
    m.write(GuestPtr(0x1080 + 52), &[255, 255, 255, 255, 0, 0])
        .unwrap();

    assert_ne!(h, 0);
    assert_eq!(m.u16(GuestPtr(0x1080 + 66)).unwrap() as i16, -3);
    assert_eq!(m.u32(GuestPtr(0x1080 + 100)).unwrap(), h);
    assert!(op(&mut m, &mut p, None, Op::Open).is_err());
    assert!(line(&mut m, &mut p, None, 10, 2).unwrap());
    assert_eq!(bytes(&m, &mut p, h).len(), 18);
    ports.pen_normal(&mut m).unwrap();
    m.write(GuestPtr(0x1080 + 100), &0u32.to_be_bytes())
        .unwrap();
    assert!(!line(&mut m, &mut p, None, 4, 4).unwrap());
    ports.line(&mut m, None, 4, 4, false).unwrap();
    position(&mut m, 10, 2);
    m.write(GuestPtr(0x1080 + 100), &h.to_be_bytes()).unwrap();
    assert!(line(&mut m, &mut p, None, 2, 10).unwrap());
    assert!(op(&mut m, &mut p, None, Op::Kill(h)).is_err());
    ports.pen_visibility(&mut m, 1).unwrap();
    ports.pen_visibility(&mut m, -1).unwrap();
    op(&mut m, &mut p, None, Op::Close).unwrap();
    assert_eq!(m.u16(GuestPtr(0x1080 + 66)).unwrap() as i16, -2);
    assert_eq!(m.u32(GuestPtr(0x1080 + 100)).unwrap(), 0);
    assert_eq!(m.read(GuestPtr(0x1080 + 48), 4).unwrap(), [0, 10, 0, 2]);
    assert_eq!(
        bytes(&m, &mut p, h),
        support::words(&[22, 2, 2, 10, 10, 2, 2, 2, 10, 10, 2])
    );
    assert_eq!(m.read(GuestPtr(0x2000), 32).unwrap(), [0x5a; 32]);
    ports.pen_normal(&mut m).unwrap();
    op(
        &mut m,
        &mut p,
        None,
        Op::Draw {
            handle: h,
            verb: Verb::Frame,
        },
    )
    .unwrap();
    op(
        &mut m,
        &mut p,
        None,
        Op::Offset {
            handle: h,
            dh: 1,
            dv: -1,
        },
    )
    .unwrap();
    assert_eq!(&bytes(&m, &mut p, h)[2..10], &[0, 1, 0, 3, 0, 9, 0, 11]);
    op(&mut m, &mut p, None, Op::Kill(h)).unwrap();
    assert_eq!(m.u32(GuestPtr(h)).unwrap(), 0);
}
#[test]
fn polygon_growth_failure_and_malformed_records_preserve_pen_and_payload() {
    let (mut m, mut p, _) = setup();
    let h = op(&mut m, &mut p, None, Op::Open).unwrap();
    let blocker = p.new_pointer(&mut m, 16, false).unwrap();
    p.set_handle_lock(&m, h, true).unwrap();
    let before = bytes(&m, &mut p, h);
    assert!(line(&mut m, &mut p, None, 10, 10).unwrap());
    assert_eq!(p.mem_error, -108);
    assert_eq!(bytes(&m, &mut p, h), before);
    assert_eq!(m.read(GuestPtr(0x1080 + 48), 4).unwrap(), [0; 4]);
    p.set_handle_lock(&m, h, false).unwrap();
    p.dispose_pointer(blocker);
    line(&mut m, &mut p, None, 10, 10).unwrap();
    op(&mut m, &mut p, None, Op::Close).unwrap();
    let ptr = GuestPtr(m.u32(GuestPtr(h)).unwrap());
    m.write(ptr, &11u16.to_be_bytes()).unwrap();
    assert!(op(
        &mut m,
        &mut p,
        None,
        Op::Draw {
            handle: h,
            verb: Verb::Paint
        }
    )
    .is_err());
    assert_eq!(m.read(GuestPtr(0x2000), 32).unwrap(), [0; 32]);

    let (mut m, mut p, ports) = setup();
    let h = op(&mut m, &mut p, None, Op::Open).unwrap();
    let before = bytes(&m, &mut p, h);
    ports.pen_visibility(&mut m, 1).unwrap();
    assert!(line(&mut m, &mut p, None, 5, 5).is_err());
    assert_eq!(bytes(&m, &mut p, h), before);
    assert_eq!(m.read(GuestPtr(0x1080 + 48), 4).unwrap(), [0; 4]);
    op(&mut m, &mut p, None, Op::Close).unwrap();
    let (mut m, mut p, _) = setup();
    let h = op(&mut m, &mut p, None, Op::Open).unwrap();
    p.set_handle_size(&mut m, h, 65534).unwrap();
    let base = GuestPtr(m.u32(GuestPtr(h)).unwrap());
    m.write(base, &vec![0; 65534]).unwrap();
    m.write(base, &65534u16.to_be_bytes()).unwrap();
    assert!(line(&mut m, &mut p, None, 0, 0).is_err());
    assert_eq!(p.handle_size(&m, h).unwrap(), 65534);
    assert_eq!(m.u16(base).unwrap(), 65534);
    assert_eq!(m.read(GuestPtr(0x1080 + 48), 4).unwrap(), [0; 4]);
}

#[test]
fn polygon_abis_validate_open_result_and_decode_signed_offsets_and_patterns() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        vec![0xff, 0xfe, 0, 3, 0x10, 0, 0x20, 0],
        Permissions::RW,
    )
    .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa8cb, &mut c, &m).unwrap(),
        (Call::Polygon(Op::Open), Reply::StackLong(GuestPtr(0x1000)))
    );
    assert_eq!(c.dar[15], 0x1000);
    assert_eq!(
        Abi::trap(0xa8ce, &mut c, &m).unwrap(),
        (
            Call::Polygon(Op::Offset {
                handle: 0x10002000,
                dh: 3,
                dv: -2
            }),
            Reply::None
        )
    );
    assert_eq!(c.dar[15], 0x1008);
    c.dar[15] = 0x1002;
    assert!(Abi::trap(0xa8ce, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1002);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = 0x10002000;
    c.gpr[4] = 0xabcd0003;
    c.gpr[5] = 0x1234fffe;
    assert_eq!(
        Abi::import(Service::OffsetPoly, &c, &m).unwrap().0,
        Call::Polygon(Op::Offset {
            handle: 0x10002000,
            dh: 3,
            dv: -2
        })
    );
    c.gpr[4] = 0x3000;
    assert_eq!(
        Abi::import(Service::FillPoly, &c, &m).unwrap().0,
        Call::Polygon(Op::Draw {
            handle: 0x10002000,
            verb: Verb::Fill(GuestPtr(0x3000))
        })
    );
    let mut readonly = Memory::default();
    readonly
        .map(GuestPtr(0x1000), vec![0; 4], Permissions::RX)
        .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert!(Abi::trap(0xa8cb, &mut c, &readonly).is_err());
    assert_eq!(c.dar[15], 0x1000);
}
// Independent floating-point formulation of the permitted legacy scanline rule.
fn inside(points: &[(i16, i16)], x: i32, y: i32) -> bool {
    let mut crossings = 0;
    for i in 0..points.len() {
        let (mut x0, mut y0) = (f64::from(points[i].0), f64::from(points[i].1));
        let (mut x1, mut y1) = (
            f64::from(points[(i + 1) % points.len()].0),
            f64::from(points[(i + 1) % points.len()].1),
        );
        if y0 > y1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        if y0 <= f64::from(y) && f64::from(y) < y1 {
            let at = x0 + (f64::from(y) - y0) * (x1 - x0) / (y1 - y0);
            if at <= f64::from(x) {
                crossings += 1;
            }
        }
    }
    crossings % 2 != 0
}
fn bit(m: &Memory, x: usize, y: usize) -> u8 {
    (m.read(GuestPtr(0x2000), 32).unwrap()[2 * y + x / 8] >> (7 - x % 8)) & 1
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_polygon_fills_match_reference_and_frames_union_joints_for_xor() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 16, 16, GuestPtr(0x4000))).unwrap();
    let (mut m, mut p, _) = setup();
    for points in [
        vec![(2, 2), (12, 2), (2, 12)],
        vec![(1, 1), (12, 1), (12, 6), (7, 6), (7, 13), (1, 13)],
        vec![(1, 1), (12, 12), (1, 12), (12, 1)],
        vec![(2, 12), (12, 2), (2, 2)],
    ] {
        m.write(GuestPtr(0x2000), &[0; 32]).unwrap();
        let h = record(&mut m, &mut p, &points);
        assert_eq!(m.read(GuestPtr(0x2000), 32).unwrap(), [0; 32]);
        let port = m.read(GuestPtr(0x1080), 108).unwrap().to_vec();
        p.mem_error = -117;
        op(
            &mut m,
            &mut p,
            Some(&gpu),
            Op::Draw {
                handle: h,
                verb: Verb::Paint,
            },
        )
        .unwrap();
        assert_eq!(p.mem_error, -117);
        for y in 0..16 {
            for x in 0..16 {
                assert_eq!(
                    bit(&m, x, y),
                    u8::from(inside(&points, x as i32, y as i32)),
                    "{points:?} at {x},{y}"
                );
            }
        }
        let painted = m.read(GuestPtr(0x2000), 32).unwrap().to_vec();
        op(
            &mut m,
            &mut p,
            Some(&gpu),
            Op::Draw {
                handle: h,
                verb: Verb::Invert,
            },
        )
        .unwrap();
        op(
            &mut m,
            &mut p,
            Some(&gpu),
            Op::Draw {
                handle: h,
                verb: Verb::Invert,
            },
        )
        .unwrap();
        assert_eq!(m.read(GuestPtr(0x2000), 32).unwrap(), painted);
        op(
            &mut m,
            &mut p,
            Some(&gpu),
            Op::Draw {
                handle: h,
                verb: Verb::Erase,
            },
        )
        .unwrap();
        assert_eq!(m.read(GuestPtr(0x2000), 32).unwrap(), [0; 32]);
        assert_eq!(m.read(GuestPtr(0x1080), 108).unwrap(), port);
        op(&mut m, &mut p, None, Op::Kill(h)).unwrap();
    }
    let points = [(2, 2), (10, 2), (10, 10), (2, 10)];
    let h = record(&mut m, &mut p, &points);
    m.write(GuestPtr(0x1080 + 52), &[0, 2, 0, 2, 0, 10])
        .unwrap();
    let port = m.read(GuestPtr(0x1080), 108).unwrap().to_vec();
    op(
        &mut m,
        &mut p,
        Some(&gpu),
        Op::Draw {
            handle: h,
            verb: Verb::Frame,
        },
    )
    .unwrap();
    for y in 0..16 {
        for x in 0..16 {
            let expected = (2..12).contains(&x)
                && (2..12).contains(&y)
                && (!(4..10).contains(&x) || !(4..10).contains(&y));
            assert_eq!(bit(&m, x, y), u8::from(expected), "frame {x},{y}");
        }
    }
    op(
        &mut m,
        &mut p,
        Some(&gpu),
        Op::Draw {
            handle: h,
            verb: Verb::Frame,
        },
    )
    .unwrap();
    assert_eq!(m.read(GuestPtr(0x2000), 32).unwrap(), [0; 32]);
    assert_eq!(m.read(GuestPtr(0x1080), 108).unwrap(), port);
    // A hole in the clip region combines with polygon and supplied pattern.
    let outer = support::words(&[10, 0, 0, 16, 16]);
    let hole = support::words(&[10, 4, 4, 8, 8]);
    let clip_bytes = systemless_future::region_geometry::combine(
        &outer,
        &hole,
        systemless_future::region_geometry::Boolean::Difference,
    )
    .unwrap();
    let clip = m.u32(GuestPtr(0x1080 + 28)).unwrap();
    p.set_handle_size(&mut m, clip, clip_bytes.len() as u32)
        .unwrap();
    m.write(GuestPtr(m.u32(GuestPtr(clip)).unwrap()), &clip_bytes)
        .unwrap();
    let pattern = [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55];
    m.map(GuestPtr(0x1300), pattern.to_vec(), Permissions::RW)
        .unwrap();
    op(
        &mut m,
        &mut p,
        Some(&gpu),
        Op::Draw {
            handle: h,
            verb: Verb::Fill(GuestPtr(0x1300)),
        },
    )
    .unwrap();
    for y in 0..16 {
        for x in 0..16 {
            let covered = (2..10).contains(&x)
                && (2..10).contains(&y)
                && !((4..8).contains(&x) && (4..8).contains(&y));
            let expected = if covered {
                (pattern[y % 8] >> (7 - x % 8)) & 1
            } else {
                0
            };
            assert_eq!(bit(&m, x, y), expected, "pattern/clip {x},{y}");
        }
    }
    assert_eq!(m.read(GuestPtr(0x1080 + 40), 8).unwrap(), pattern);
    p.set_handle_size(&mut m, clip, 10).unwrap();
    m.write(GuestPtr(m.u32(GuestPtr(clip)).unwrap()), &outer)
        .unwrap();
    op(&mut m, &mut p, None, Op::Kill(h)).unwrap();
    m.write(GuestPtr(0x2000), &[0; 32]).unwrap();
    m.write(GuestPtr(0x1080 + 52), &[0, 1, 0, 1, 0, 8]).unwrap();
    position(&mut m, 1, 1);
    let visible = op(&mut m, &mut p, None, Op::Open).unwrap();
    p.ports.unwrap().pen_visibility(&mut m, 1).unwrap();
    assert!(line(&mut m, &mut p, Some(&gpu), 7, 1).unwrap());
    for y in 0..16 {
        for x in 0..16 {
            assert_eq!(
                bit(&m, x, y),
                u8::from(y == 1 && (1..=7).contains(&x)),
                "visible record {x},{y}"
            );
        }
    }
    op(&mut m, &mut p, None, Op::Close).unwrap();
    assert_eq!(m.u16(GuestPtr(0x1080 + 66)).unwrap(), 1);
    p.ports.unwrap().pen_visibility(&mut m, -1).unwrap();
    op(&mut m, &mut p, None, Op::Kill(visible)).unwrap();
    m.write(GuestPtr(0x2000), &[0; 32]).unwrap();
    let bounded = gpu.target(128, 128).unwrap();
    m.map(bounded.base, vec![7; 128 * 128], Permissions::RW)
        .unwrap();
    let many = (0..16381)
        .map(|i| [(0, 0), (127, 0), (0, 127)][i % 3])
        .collect::<Vec<_>>();
    assert!(bounded
        .execute(
            &mut m,
            Draw::Polygon {
                rect: Rect::new(0, 0, 127, 127),
                clip: Rect::new(0, 0, 128, 128),
                vertices: &many,
                origin_x: 0,
                origin_y: 0,
                frame: None,
                pattern: [255; 8],
                foreground: 1,
                background: 0,
                mode: Transfer::Copy
            }
        )
        .is_err());
    assert_eq!(m.read(bounded.base, 16).unwrap(), [7; 16]);
    // Origin translation must preserve the guest pattern phase.
    let points = [(-5, -3), (5, -3), (-5, 7)];
    let pattern = [0x80, 0x40, 0x20, 0x10, 8, 4, 2, 1];
    let bitmap = systemless_future::bitmap::Bitmap {
        base: GuestPtr(0x2000),
        row_bytes: 2,
        bounds: Rect::new(-7, -5, 9, 11),
    };
    bitmap
        .execute(
            &gpu,
            &mut m,
            Draw::Polygon {
                rect: Rect::new(-5, -3, 5, 7),
                clip: bitmap.bounds,
                vertices: &points,
                origin_x: 0,
                origin_y: 0,
                frame: None,
                pattern,
                foreground: 1,
                background: 0,
                mode: Transfer::Copy,
            },
        )
        .unwrap();
    for y in 0..16 {
        for x in 0..16 {
            let (gx, gy) = (x as i32 - 7, y as i32 - 5);
            let expected = if inside(&points, gx, gy) {
                (pattern[(gy & 7) as usize] >> (7 - (gx & 7))) & 1
            } else {
                0
            };
            assert_eq!(bit(&m, x, y), expected, "origin {gx},{gy}");
        }
    }
    m.write(GuestPtr(0x2000), &[0; 32]).unwrap();
    let extreme = [
        (i16::MIN, i16::MIN),
        (i16::MIN + 10, i16::MIN),
        (i16::MIN, i16::MIN + 10),
    ];
    let bitmap = systemless_future::bitmap::Bitmap {
        base: GuestPtr(0x2000),
        row_bytes: 2,
        bounds: Rect::new(-32768, -32768, -32752, -32752),
    };
    bitmap
        .execute(
            &gpu,
            &mut m,
            Draw::Polygon {
                rect: Rect::new(-32768, -32768, -32758, -32758),
                clip: bitmap.bounds,
                vertices: &extreme,
                origin_x: 0,
                origin_y: 0,
                frame: None,
                pattern: [255; 8],
                foreground: 1,
                background: 0,
                mode: Transfer::Copy,
            },
        )
        .unwrap();
    for y in 0..16 {
        for x in 0..16 {
            assert_eq!(
                bit(&m, x, y),
                u8::from(inside(&extreme, x as i32 - 32768, y as i32 - 32768)),
                "extreme origin {x},{y}"
            );
        }
    }
}
