use std::env;
use std::fs;
use std::path::PathBuf;
use stuffit::{SitArchive, SitEntry};

fn argument(name: &str) -> PathBuf {
    env::args_os()
        .nth(match name {
            "data fork" => 1,
            "resource fork" => 2,
            "archive" => 3,
            _ => unreachable!(),
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing {name} path"))
}

fn main() {
    let data_path = argument("data fork");
    let resource_path = argument("resource fork");
    let archive_path = argument("archive");

    if env::args_os().nth(4).is_some() {
        panic!("usage: toolbox-showcase-packer DATA RESOURCE ARCHIVE");
    }

    let mut archive = SitArchive::new();
    archive.add_entry(SitEntry {
        name: "Toolbox Showcase".to_owned(),
        data_fork: fs::read(&data_path).expect("read PowerPC data fork"),
        resource_fork: fs::read(&resource_path).expect("read resource fork"),
        file_type: *b"APPL",
        creator: *b"SHWC",
        ..SitEntry::default()
    });

    let bytes = archive
        .serialize()
        .expect("serialize deterministic StuffIt 5 archive");
    fs::write(&archive_path, bytes).expect("write StuffIt archive");
}
