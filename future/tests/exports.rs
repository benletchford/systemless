mod support;
use support::{pef, w16, w32, Pef};
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE, STACK_SIZE},
    memory::GuestPtr,
    runtime::Stop,
};

struct Symbol {
    name: &'static [u8],
    key: u32,
    class: u8,
    section: i16,
    value: u32,
}
struct Fixture {
    pef: Pef,
    hashes: usize,
    keys: usize,
    entries: usize,
    names: Vec<&'static [u8]>,
}
impl Fixture {
    fn index(&self, name: &[u8]) -> usize {
        self.names.iter().position(|n| *n == name).unwrap()
    }
}
fn fixture() -> Fixture {
    // Fixed hash words derived from Apple's PEF algorithm, rather than calling
    // the production hasher to manufacture the data it must validate.
    let mut specs = vec![
        Symbol {
            name: b"main",
            key: 0x00040250,
            class: 2,
            section: 1,
            value: 0,
        },
        Symbol {
            name: b"Main",
            key: 0x00040350,
            class: 0,
            section: 0,
            value: 0,
        },
        Symbol {
            name: b"data",
            key: 0x0004022d,
            class: 1,
            section: 1,
            value: 16,
        },
        Symbol {
            name: b"abs",
            key: 0x00030133,
            class: 1,
            section: -2,
            value: 0xdead_beef,
        },
        Symbol {
            name: b"ticks",
            key: 0x00050521,
            class: 2,
            section: -3,
            value: 0,
        },
        Symbol {
            name: b"\x80x",
            key: 0x00020178,
            class: 1,
            section: 1,
            value: 20,
        },
        Symbol {
            name: b"end",
            key: 0x0003012c,
            class: 1,
            section: 1,
            value: 64,
        },
        Symbol {
            name: b"veryLongNameForSignedHashOverflow",
            key: 0x002158f8,
            class: 3,
            section: 1,
            value: 0,
        },
        Symbol {
            name: b"glue",
            key: 0x00040207,
            class: 4,
            section: 0,
            value: 0,
        },
    ];
    let slot = |s: &Symbol| ((s.key ^ (s.key >> 2)) & 3) as usize;
    specs.sort_by_key(slot);
    let mut f = pef();
    let old_start =
        u32::from_be_bytes(f.bytes[f.loader + 44..f.loader + 48].try_into().unwrap()) as usize;
    f.bytes.truncate(f.loader + old_start);
    let mut offsets = Vec::new();
    for s in &specs {
        offsets.push((f.bytes.len() - f.strings) as u32);
        f.bytes.extend(s.name); // deliberately not NUL-terminated
    }
    while !(f.bytes.len() - f.loader).is_multiple_of(4) {
        f.bytes.push(0);
    }
    let hashes = f.bytes.len();
    let keys = hashes + 16;
    let entries = keys + specs.len() * 4;
    f.bytes.resize(entries + specs.len() * 10, 0);
    w32(&mut f.bytes, f.loader + 44, (hashes - f.loader) as u32);
    w32(&mut f.bytes, f.loader + 48, 2);
    w32(&mut f.bytes, f.loader + 52, specs.len() as u32);
    for bucket in 0..4 {
        let first = specs.iter().position(|s| slot(s) == bucket).unwrap();
        let count = specs.iter().filter(|s| slot(s) == bucket).count();
        w32(
            &mut f.bytes,
            hashes + bucket * 4,
            ((count as u32) << 18) | first as u32,
        );
    }
    for (i, s) in specs.iter().enumerate() {
        w32(&mut f.bytes, keys + i * 4, s.key);
        w32(
            &mut f.bytes,
            entries + i * 10,
            (u32::from(s.class) << 24) | offsets[i],
        );
        w32(&mut f.bytes, entries + i * 10 + 4, s.value);
        w16(&mut f.bytes, entries + i * 10 + 8, s.section as u16);
    }
    let loader_size = (f.bytes.len() - f.loader) as u32;
    for field in [8, 12, 16] {
        w32(&mut f.bytes, 40 + 2 * 28 + field, loader_size);
    }
    Fixture {
        pef: f,
        hashes,
        keys,
        entries,
        names: specs.iter().map(|s| s.name).collect(),
    }
}

#[test]
fn exports_preserve_table_order_classes_raw_names_and_resolve_all_address_kinds() {
    let f = fixture();
    let app = LoadedApplication::pef(&f.pef.bytes, None).unwrap();
    let code = app.info.sections[0].0;
    let data = app.info.sections[1].0;
    assert_eq!(
        app.info
            .exports
            .iter()
            .map(|e| e.name.as_slice())
            .collect::<Vec<_>>(),
        f.names
    );
    for (name, class, address) in [
        (b"main".as_slice(), 2, data),
        (b"Main", 0, code),
        (b"data", 1, data + 16),
        (b"abs", 1, 0xdead_beef),
        (
            b"ticks",
            2,
            app.runtime.memory.u32(GuestPtr(data + 8)).unwrap(),
        ),
        (b"\x80x", 1, data + 20),
        (b"end", 1, data + 64),
        (b"veryLongNameForSignedHashOverflow", 3, data),
        (b"glue", 4, code),
    ] {
        let e = &app.info.exports[f.index(name)];
        assert_eq!((e.class, e.address), (class, GuestPtr(address)));
    }
}

#[test]
fn exported_transition_vector_executes_through_the_real_ppc_backend() {
    let f = fixture();
    let mut app = LoadedApplication::pef(&f.pef.bytes, None).unwrap();
    let vector = app.info.exports[f.index(b"main")].address;
    let entry = app.runtime.memory.u32(vector).unwrap();
    let toc = app.runtime.memory.u32(GuestPtr(vector.0 + 4)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, entry, STACK_BASE + STACK_SIZE - 256);
    if let Cpu::PowerPc(cpu) = &mut app.runtime.cpu {
        cpu.gpr[2] = toc;
    }
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(app.runtime.services.calls.len(), 2);
}

#[test]
fn malformed_export_addresses_names_hashes_and_ownership_cannot_launch() {
    let f = fixture();
    let at = |name: &[u8]| f.entries + f.index(name) * 10;
    let mut cases = Vec::new();
    for (p, value) in [
        (at(b"data") + 4, 65),
        (at(b"ticks") + 4, 2),
        (f.keys, 0x00040251),
        (f.hashes, 0),           // orphaned symbols
        (f.hashes + 4, 1 << 18), // duplicate ownership of symbol zero
        (f.pef.loader + 48, 31),
        (f.pef.loader + 52, u32::MAX),
    ] {
        let mut b = f.pef.bytes.clone();
        w32(&mut b, p, value);
        cases.push(b);
    }
    for section in [-1i16, -4, 2, 32767] {
        let mut b = f.pef.bytes.clone();
        w16(&mut b, at(b"data") + 8, section as u16);
        cases.push(b);
    }
    let mut b = f.pef.bytes.clone();
    b[at(b"main")] = 0x82;
    cases.push(b);
    let mut b = f.pef.bytes.clone();
    w32(&mut b, at(b"main"), 0x02ff_ffff);
    cases.push(b);
    let mut b = f.pef.bytes.clone();
    let source = b[at(b"main")..at(b"main") + 4].to_vec();
    b[at(b"Main")..at(b"Main") + 4].copy_from_slice(&source);
    w32(&mut b, f.keys + f.index(b"Main") * 4, 0x00040250);
    cases.push(b); // duplicate name
    for (i, b) in cases.into_iter().enumerate() {
        assert!(
            LoadedApplication::pef(&b, None).is_err(),
            "accepted mutation {i}"
        );
    }
}
