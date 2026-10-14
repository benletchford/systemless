//! Emit reproducible format-valid applications used by the loader contracts.
#[path = "../tests/support/mod.rs"]
mod support;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: make_load_fixtures OUTPUT_DIRECTORY")?;
    let directory = std::path::Path::new(&path);
    std::fs::create_dir_all(directory)?;
    std::fs::write(directory.join("resources.rsrc"), support::resource_code())?;
    std::fs::write(
        directory.join("application.pef"),
        support::resource_pef().bytes,
    )?;
    println!(
        "wrote CODE/resource fork and PEF applications to {}",
        directory.display()
    );
    Ok(())
}
