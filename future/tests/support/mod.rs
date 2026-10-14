#![allow(dead_code)]
// Reproducible binary layouts; no dependency on the production parsers.
pub fn w16(b: &mut [u8], p: usize, v: u16) {
    b[p..p + 2].copy_from_slice(&v.to_be_bytes());
}
pub fn w32(b: &mut [u8], p: usize, v: u32) {
    b[p..p + 4].copy_from_slice(&v.to_be_bytes());
}
pub fn words(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_be_bytes()).collect()
}
pub type Entry = ([u8; 4], i16, Option<Vec<u8>>, Vec<u8>);
pub fn resource(entries: Vec<Entry>) -> Vec<u8> {
    let mut grouped = std::collections::BTreeMap::<[u8; 4], Vec<_>>::new();
    for e in entries {
        grouped.entry(e.0).or_default().push(e);
    }
    let mut data = Vec::new();
    let mut names = Vec::new();
    let mut records = Vec::new();
    let types = 36; // deliberately non-default offset, to detect hardcoded 28.
    let refs_start = types + 2 + grouped.len() * 8;
    let mut types_data = vec![0u8; 2 + grouped.len() * 8];
    w16(&mut types_data, 0, (grouped.len() as u16).wrapping_sub(1));
    for (i, (kind, entries)) in grouped.iter().enumerate() {
        let t = 2 + i * 8;
        types_data[t..t + 4].copy_from_slice(kind);
        w16(&mut types_data, t + 4, entries.len() as u16 - 1);
        w16(
            &mut types_data,
            t + 6,
            (refs_start + records.len() - types) as u16,
        );
        for (_, id, name, contents) in entries {
            let mut record = vec![0; 12];
            w16(&mut record, 0, *id as u16);
            let name_offset = if let Some(name) = name {
                let p = names.len() as u16;
                names.push(name.len() as u8);
                names.extend(name);
                p
            } else {
                0xffff
            };
            w16(&mut record, 2, name_offset);
            w32(&mut record, 4, data.len() as u32);
            data.extend((contents.len() as u32).to_be_bytes());
            data.extend(contents);
            records.extend(record);
        }
    }
    let names_at = refs_start + records.len();
    let mut map = vec![0; types];
    w16(&mut map, 24, types as u16);
    w16(&mut map, 26, names_at as u16);
    map.extend(types_data);
    map.extend(records);
    map.extend(names);
    let mut out = vec![0; 256];
    w32(&mut out, 0, 256);
    w32(&mut out, 4, (256 + data.len()) as u32);
    w32(&mut out, 8, data.len() as u32);
    w32(&mut out, 12, map.len() as u32);
    map[..16].copy_from_slice(&out[..16]);
    out.extend(data);
    out.extend(map);
    out
}
pub fn code() -> Vec<u8> {
    let mut zero = vec![0; 32];
    for (p, v) in [(0, 48), (4, 256), (8, 16), (12, 32)] {
        w32(&mut zero, p, v);
    }
    zero[16..].copy_from_slice(&words(&[0, 0x3f3c, 1, 0xa9f0, 0, 0x3f3c, 2, 0xa9f0]));
    let main = words(&[
        0, 1, 0x598f, 0xa975, 0x201f, 0x2b40, 0xfffc, 0x4ead, 0x002a, 0xa9f4,
    ]);
    let other = words(&[8, 1, 0x2b7c, 0x1234, 0x5678, 0xfff8, 0x4e75]); // store constant to A5-8, RTS
    resource(vec![
        (*b"CODE", 0, None, zero),
        (*b"CODE", 1, None, main),
        (*b"CODE", 2, None, other),
        (*b"DATA", -7, Some(vec![0x80, b'x']), vec![1, 2, 3]),
    ])
}
pub struct Pef {
    pub bytes: Vec<u8>,
    pub loader: usize,
    pub data: usize,
    pub reloc: usize,
    pub strings: usize,
}
pub fn pef() -> Pef {
    let instructions = [
        0x7fc802a6u32,
        0x7c5f1378,
        0x81820008,
        0x800c0000,
        0x804c0004,
        0x7c0903a6,
        0x4e800421,
        0x907f0010,
        0x7fe2fb78,
        0x8182000c,
        0x800c0000,
        0x804c0004,
        0x7c0903a6,
        0x4e800421,
    ];
    pef_program(&instructions, &[b"TickCount", b"ExitToShell"], 24)
}
pub fn pef_program(instructions: &[u32], imports: &[&[u8]], data_size: usize) -> Pef {
    let code: Vec<_> = instructions.iter().flat_map(|w| w.to_be_bytes()).collect();
    let data = vec![0; data_size];
    let symbols_at = 80;
    let reloc_header = symbols_at + imports.len() * 4;
    let reloc_at = reloc_header + 12;
    let relocations = if imports.is_empty() {
        words(&[0x4600])
    } else {
        words(&[0x4600, 0x4a00 | (imports.len() as u16 - 1)])
    };
    let strings_at = reloc_at + relocations.len();
    let mut strings = b"InterfaceLib\0".to_vec();
    let mut loader = vec![0; reloc_at];
    w32(&mut loader, 0, 1);
    w32(&mut loader, 8, u32::MAX);
    w32(&mut loader, 16, u32::MAX);
    w32(&mut loader, 24, 1);
    w32(&mut loader, 28, imports.len() as u32);
    w32(&mut loader, 32, 1);
    w32(&mut loader, 36, reloc_at as u32);
    w32(&mut loader, 40, strings_at as u32);
    w32(&mut loader, 68, imports.len() as u32);
    for (i, name) in imports.iter().enumerate() {
        w32(
            &mut loader,
            symbols_at + i * 4,
            0x02000000 | strings.len() as u32,
        );
        strings.extend(*name);
        strings.push(0);
    }
    w16(&mut loader, reloc_header, 1);
    w32(
        &mut loader,
        reloc_header + 4,
        (relocations.len() / 2) as u32,
    );
    loader.extend(relocations);
    loader.extend(strings);
    while !loader.len().is_multiple_of(4) {
        loader.push(0);
    }
    let export = loader.len();
    w32(&mut loader, 44, export as u32);
    loader.extend([0; 4]);
    let mut out = vec![0; 128];
    out[..12].copy_from_slice(b"Joy!peffpwpc");
    w32(&mut out, 12, 1);
    w16(&mut out, 32, 3);
    w16(&mut out, 34, 2);
    let mut offsets = Vec::new();
    for (i, (kind, contents)) in [(0, code), (1, data), (4, loader)].into_iter().enumerate() {
        while !out.len().is_multiple_of(16) {
            out.push(0);
        }
        let at = out.len();
        offsets.push(at);
        let h = 40 + i * 28;
        w32(&mut out, h, u32::MAX);
        w32(
            &mut out,
            h + 8,
            contents.len() as u32 + if kind == 1 { 40 } else { 0 },
        );
        w32(&mut out, h + 12, contents.len() as u32);
        w32(&mut out, h + 16, contents.len() as u32);
        w32(&mut out, h + 20, at as u32);
        out[h + 24] = kind;
        out[h + 25] = 1;
        out[h + 26] = 4;
        out.extend(contents);
    }
    Pef {
        loader: offsets[2],
        data: offsets[1],
        reloc: offsets[2] + reloc_at,
        strings: offsets[2] + strings_at,
        bytes: out,
    }
}
pub fn code_program(instructions: &[u16]) -> Vec<u8> {
    let mut zero = vec![0; 24];
    for (p, v) in [(0, 40), (4, 256), (8, 8), (12, 32)] {
        w32(&mut zero, p, v);
    }
    zero[16..].copy_from_slice(&words(&[0, 0x3f3c, 1, 0xa9f0]));
    let mut code = words(&[0, 1]);
    code.extend(words(instructions));
    resource(vec![
        (*b"CODE", 0, None, zero),
        (*b"CODE", 1, None, code),
        (*b"DATA", -7, None, vec![1, 2, 3]),
    ])
}
pub fn resource_pef() -> Pef {
    let mut code = vec![0x7fc802a6, 0x7c5f1378];
    fn call(code: &mut Vec<u32>, index: u32) {
        code.extend([
            0x819f0000 | (8 + index * 4),
            0x800c0000,
            0x804c0004,
            0x7c0903a6,
            0x4e800421,
        ]);
    }
    code.extend([0x3c604441, 0x60635441, 0x3880fff9]);
    call(&mut code, 0);
    code.extend([0x907f0040, 0x80a30000, 0x88c50000, 0x90df0044]);
    call(&mut code, 1);
    code.push(0x907f0048);
    code.extend([0x3c604441, 0x60635441, 0x3880fff9]);
    call(&mut code, 0);
    code.push(0x907f004c);
    code.push(0x3860000c);
    call(&mut code, 2);
    code.push(0x907f0050);
    call(&mut code, 3);
    pef_program(
        &code,
        &[
            b"GetResource",
            b"GetHandleSize",
            b"NewHandleClear",
            b"ExitToShell",
        ],
        96,
    )
}
pub fn resource_code() -> Vec<u8> {
    code_program(&[
        0x598f, 0x2f3c, 0x4441, 0x5441, 0x3f3c, 0xfff9, 0xa9a0, 0x205f, // GetResource -> A0
        0x2b48, 0xfffc, 0x2250, 0x7000, 0x1011, 0x2b40, 0xfff8, // handle & byte to A5 globals
        0xa025, 0x2b40, 0xfff4, // GetHandleSize
        0x598f, 0x2f3c, 0x4441, 0x5441, 0x3f3c, 0xfff9, 0xa9a0, 0x205f, 0x2b48, 0xfff0, 0x700c,
        0xa322, 0x2b48, 0xffec, 0xa9f4,
    ])
}
