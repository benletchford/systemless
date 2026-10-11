fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let root = "src/systems/macintosh/quickdraw/fonts/outline/monochrome";
    println!("cargo:rerun-if-changed={root}");
    cc::Build::new()
        .include(format!("{root}/freetype"))
        .file(format!("{root}/freetype/ftraster.c"))
        .file(format!("{root}/bridge.c"))
        .define("STANDALONE_", None)
        .define("ft_standard_raster", "systemless_ft_standard_raster")
        .define("FT_Outline_Get_CBox", "systemless_ft_outline_control_box")
        .compile("systemless_mono_raster");
}
