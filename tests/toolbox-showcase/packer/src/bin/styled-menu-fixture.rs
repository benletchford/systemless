//! Derive a menu-style fixture without rebuilding or modifying guest code.
use std::{env, fs};
use stuffit::{SitArchive, SitEntry};

fn word(bytes: &[u8], at: usize) -> usize {
    u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}
fn long(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

fn style_pages(resource: &mut [u8]) -> Vec<usize> {
    let data = long(resource, 0);
    let map = long(resource, 4);
    let types = map + word(resource, map + 24);
    let mut found = None;
    for index in 0..=word(resource, types) {
        let entry = types + 2 + index * 8;
        if &resource[entry..entry + 4] != b"MENU" { continue; }
        let refs = types + word(resource, entry + 6);
        for item in 0..=word(resource, entry + 4) {
            let reference = refs + item * 12;
            if word(resource, reference) != 129 { continue; }
            assert!(found.is_none(), "unique Pages MENU resource");
            let offset = long(resource, reference + 4) & 0x00ff_ffff;
            let start = data + offset + 4;
            found = Some((start, start + long(resource, data + offset)));
        }
    }
    let (start, end) = found.expect("Pages MENU129 resource");
    assert_eq!(word(resource, start), 129);
    assert_eq!(&resource[start + 15..start + 20], b"Pages");
    let mut cursor = start + 14 + 1 + resource[start + 14] as usize;
    let mut changed = Vec::new();
    let mut count = 0;
    while resource[cursor] != 0 {
        let style = cursor + 1 + resource[cursor] as usize + 3;
        assert!(style < end);
        assert_eq!(resource[style], 0, "base fixture items must be plain");
        let face = (count % 8) as u8;
        resource[style] = face;
        if face != 0 { changed.push(style); }
        count += 1;
        cursor = style + 1;
    }
    assert_eq!(count, 16);
    assert_eq!(cursor + 1, end);
    changed
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    assert_eq!(args.len(), 3, "usage: styled-menu-fixture INPUT.sit OUTPUT.sit");
    let source = SitArchive::parse(&fs::read(&args[1]).unwrap()).unwrap();
    let mut output = SitArchive::new();
    let mut applications = 0;
    for entry in &source.entries {
        let (data, mut resource) = entry.decompressed_forks().unwrap();
        if entry.name == "Toolbox Showcase" {
            applications += 1;
            let original = resource.clone();
            let changed = style_pages(&mut resource);
            let actual: Vec<_> = original.iter().zip(&resource).enumerate()
                .filter_map(|(at, (a, b))| (a != b).then_some(at)).collect();
            assert_eq!(actual, changed, "only fourteen style bytes may change");
        }
        output.add_entry(SitEntry { name: entry.name.clone(), data_fork: data,
            resource_fork: resource, file_type: entry.file_type, creator: entry.creator,
            is_folder: entry.is_folder, finder_flags: entry.finder_flags,
            ..Default::default() });
    }
    assert_eq!(applications, 1);
    let serialized = output.serialize().unwrap();
    let parsed = SitArchive::parse(&serialized).unwrap();
    assert_eq!(parsed.entries.len(), output.entries.len());
    for (expected, actual) in output.entries.iter().zip(&parsed.entries) {
        assert_eq!(actual.name, expected.name);
        assert_eq!(actual.file_type, expected.file_type);
        assert_eq!(actual.creator, expected.creator);
        assert_eq!(actual.is_folder, expected.is_folder);
        assert_eq!(actual.finder_flags, expected.finder_flags);
        assert_eq!(actual.decompressed_forks().unwrap(),
            (expected.data_fork.clone(), expected.resource_fork.clone()));
    }
    fs::write(&args[2], serialized).unwrap();
}
