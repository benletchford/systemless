use std::env;
use std::fs;
use std::path::Path;
use stuffit::{SitArchive, SitEntry};

const PEF_TAGS: &[u8; 8] = b"Joy!peff";
const PEF_TIMESTAMP_OFFSET: usize = 16;
const FIXED_PEF_TIMESTAMP: u32 = 0xD2_56_A3_5A;

fn read(path: &Path, kind: &str) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| {
        panic!("failed to read {kind} {}: {error}", path.display())
    })
}

fn normalize_pef_timestamp(data_fork: &mut [u8]) {
    if data_fork.get(..PEF_TAGS.len()) == Some(PEF_TAGS.as_slice()) {
        data_fork
            .get_mut(PEF_TIMESTAMP_OFFSET..PEF_TIMESTAMP_OFFSET + 4)
            .expect("truncated PEF header")
            .copy_from_slice(&FIXED_PEF_TIMESTAMP.to_be_bytes());
    }
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() != 4 {
        eprintln!("usage: toolbox-showcase-packager <data-fork> <resource-fork> <output.sit>");
        std::process::exit(2);
    }

    let mut data_fork = read(Path::new(&args[1]), "data fork");
    let resource_fork = read(Path::new(&args[2]), "resource fork");
    normalize_pef_timestamp(&mut data_fork);

    let mut archive = SitArchive::new();
    archive.add_entry(SitEntry {
        name: "Toolbox Showcase".to_string(),
        data_fork,
        resource_fork,
        file_type: *b"APPL",
        creator: *b"SLSH",
        finder_flags: 0x0100,
        ..SitEntry::default()
    });

    let encoded = archive
        .serialize()
        .expect("serialize classic StuffIt archive");
    fs::write(&args[3], encoded).unwrap_or_else(|error| {
        panic!("failed to write {}: {error}", Path::new(&args[3]).display())
    });
}
