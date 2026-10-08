use crate::docker::{run_mpw, run_simplerez};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

fn find_common_headers_root(start_dir: &Path) -> Option<PathBuf> {
    let mut search_dir = start_dir;
    loop {
        if search_dir.join("common").join("common.h").exists() {
            return Some(search_dir.to_path_buf());
        }

        let fixtures_dir = search_dir.join("fixtures");
        if fixtures_dir.join("common").join("common.h").exists() {
            return Some(fixtures_dir);
        }

        search_dir = search_dir.parent()?;
    }
}

/// Generate MPW build script and run mps container to compile
pub fn compile_with_mpw(test_name: &str, source_dir: &Path, output_dir: &Path, work_dir: &Path) {
    println!("[{}] Preparing MPW build environment (mps)...", test_name);

    // Ensure output directory exists
    if !output_dir.exists() {
        fs::create_dir_all(output_dir).expect("Failed to create output directory");
    }

    // Use a clean workspace in the work_dir (usually crates/fixgen/mpw_workspace)
    let workspace = work_dir.join(test_name);
    if workspace.exists() {
        fs::remove_dir_all(&workspace).ok();
    }
    fs::create_dir_all(&workspace).expect("Failed to create workspace");

    let fixture_dir = workspace.join("fixture");
    let common_dir = workspace.join("common");
    fs::create_dir_all(&fixture_dir).ok();
    fs::create_dir_all(&common_dir).ok();

    // 1. Copy source files
    let src_main = source_dir.join("main.c");
    let dst_main = fixture_dir.join("main.c");
    fs::copy(&src_main, &dst_main).expect("Failed to copy main.c");

    // Copy any .r files
    if let Ok(entries) = fs::read_dir(source_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension() == Some(OsStr::new("r")) {
                let filename = path.file_name().unwrap();
                fs::copy(&path, fixture_dir.join(filename)).expect("Failed to copy .r file");
            }
        }
    }

    let common_headers_root = source_dir.parent().and_then(find_common_headers_root);
    if let Some(fixtures_root) = common_headers_root.as_ref() {
        let common_src_dir = fixtures_root.join("common");
        for entry in (fs::read_dir(&common_src_dir).expect("Failed to read common dir")).flatten() {
            let path = entry.path();
            if path.extension() == Some(OsStr::new("h")) {
                let filename = path.file_name().expect("Missing header name");
                fs::copy(&path, common_dir.join(filename)).expect("Failed to copy common header");
            }
        }
    } else {
        println!(
            "[{}] No shared common headers found under {}; building source-only app",
            test_name,
            source_dir.display()
        );
    }

    // 2. Patch main.c for MPW
    // - Add QDGlobals qd;
    // - Change include to HFS style ::common:common.h
    let main_c_content = fs::read_to_string(&dst_main).expect("Failed to read main.c");
    let mut patched = main_c_content.replace(
        "#include \"../common/common.h\"",
        "#include \"::common:common.h\"",
    );
    patched = patched.replace(
        "#include \"../../common/common.h\"",
        "#include \"::common:common.h\"",
    );
    patched = patched.replace(
        "#include \"../common/text_fixture.h\"",
        "#include \"::common:text_fixture.h\"",
    );
    patched = patched.replace(
        "#include \"../../common/text_fixture.h\"",
        "#include \"::common:text_fixture.h\"",
    );
    patched = patched.replace(
        "#include \"../common/text_grid.h\"",
        "#include \"::common:text_grid.h\"",
    );
    patched = patched.replace(
        "#include \"../../common/text_grid.h\"",
        "#include \"::common:text_grid.h\"",
    );

    if !patched.contains("QDGlobals qd;") {
        patched = patched.replace(
            "#include <Quickdraw.h>",
            "#include <Quickdraw.h>\n\n/* Added by FixGen for MPW */\nQDGlobals qd;",
        );
    }
    fs::write(&dst_main, patched).expect("Failed to write patched main.c");

    // 3. Generate Build Script (build.mpw)
    // We use HFS-style colons to avoid / interpretation issues in mps shell
    let common_include = if common_headers_root.is_some() {
        " -i FS:workspace:common:"
    } else {
        ""
    };
    let mut build_script = format!(
        "Directory FS:workspace:fixture:\n\
         SC -i FS:root:MPW:Interfaces:CIncludes:{common_include} main.c\n\
         Link main.c.o FS:root:MPW:Libraries:Libraries:MacRuntime.o FS:root:MPW:Libraries:Libraries:Interface.o FS:root:MPW:Libraries:CLibraries:StdCLib.o -o FixtureGen -t 'APPL' -c '????'\n"
    );

    // If main.r exists, compile and append resources
    if fixture_dir.join("main.r").exists() {
        // Rez -i ... -o FixtureGen main.r -a
        build_script.push_str("Rez -i FS:root:MPW:Interfaces:RIncludes: -o FixtureGen main.r -a\n");
    }

    let script_path = workspace.join("build.mpw");
    fs::write(&script_path, build_script).expect("Failed to write build.mpw");

    // 4. Run MPW build via mps
    println!("[{}] Running MPW build...", test_name);
    run_mpw(&workspace, &["%%%", "/workspace/build.mpw"]);

    // 5. Convert .rdump to binary resources using SimpleRez
    println!("[{}] Converting resources...", test_name);
    let rdump_path = "fixture/FixtureGen.rdump";
    let raw_res_path = "FixtureGen.raw_res";
    run_simplerez(&workspace, rdump_path, raw_res_path);

    // 6. Copy final artifact to output (raw res)
    let src_res = workspace.join(raw_res_path);
    let final_res = output_dir.join("FixtureGen.raw_res");

    if src_res.exists() {
        // Copy raw resource for inspection
        fs::copy(&src_res, &final_res).expect("Failed to copy result");

        // 7. Package into AppleDouble for BasiliskII (extfs)
        // This creates a real Mac application directory structure that extfs understands.
        let rsrc_data = fs::read(&src_res).expect("Failed to read resource data");

        let mac_rsrc_dir = output_dir.join(".rsrc");
        let mac_finf_dir = output_dir.join(".finf");
        fs::create_dir_all(&mac_rsrc_dir).ok();
        fs::create_dir_all(&mac_finf_dir).ok();

        // Data fork (empty for our CODE-only fixture)
        let final_app_path = output_dir.join("FixtureGen");
        fs::write(&final_app_path, []).expect("Failed to create app data fork");

        // Resource fork
        fs::write(mac_rsrc_dir.join("FixtureGen"), &rsrc_data)
            .expect("Failed to create app resource fork");

        // Finder info (32 bytes: Type [4], Creator [4], Flags [2], Location [4], etc.)
        let mut finf = vec![0u8; 32];
        finf[0..4].copy_from_slice(b"APPL");
        finf[4..8].copy_from_slice(b"????");
        fs::write(mac_finf_dir.join("FixtureGen"), &finf)
            .expect("Failed to create app finder info");

        // 8. Also package into MacBinary for the repository bin/ directory
        let final_bin_path = output_dir.join("FixtureGen.bin");
        let unused_data_fork = &[];
        let filename_bytes = b"FixtureGen";
        let file_type = *b"APPL";
        let creator = *b"????";

        let mut output_file = fs::File::create(&final_bin_path).expect("Failed to create bin file");

        crate::macbinary::encode_macbinary(
            &mut output_file,
            filename_bytes,
            &file_type,
            &creator,
            unused_data_fork,
            &rsrc_data,
        )
        .expect("Failed to encode MacBinary");

        println!(
            "[{}] SUCCESS: Generated AppleDouble and MacBinary in {}",
            test_name,
            output_dir.display()
        );
    } else {
        panic!(
            "[{}] Build failed: {} not found",
            test_name,
            src_res.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::find_common_headers_root;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "systemless_mpw_builder_{}_{}_{}",
            name,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn finds_common_headers_in_fixtures_root() {
        let root = temp_dir("fixtures_root");
        let source_dir = root.join("checkerboard");
        fs::create_dir_all(root.join("common")).unwrap();
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(root.join("common").join("common.h"), b"").unwrap();

        let found = find_common_headers_root(&source_dir);

        assert_eq!(found.as_deref(), Some(root.as_path()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finds_common_headers_in_nested_fixtures_dir() {
        let root = temp_dir("nested_fixtures");
        let source_dir = root.join("launcher_play");
        let fixtures_dir = root.join("fixtures");
        fs::create_dir_all(fixtures_dir.join("common")).unwrap();
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(fixtures_dir.join("common").join("common.h"), b"").unwrap();

        let found = find_common_headers_root(&source_dir);

        assert_eq!(found.as_deref(), Some(fixtures_dir.as_path()));
        let _ = fs::remove_dir_all(root);
    }
}
