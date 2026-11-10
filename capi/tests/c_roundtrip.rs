//! Builds `tests/roundtrip.c` with the platform's C compiler against the library this crate
//! produces, and runs it: once linked to the shared library, once to the static one.
//!
//! The C compiler is found by the `cc` crate, which also sets up MSVC's `cl` (include and
//! library paths) the way a build script would, so the same test runs on Linux, macOS and
//! Windows. `WSQ_C_SANITIZE=1` adds AddressSanitizer and UBSan (GCC and Clang only).
//!
//! `cargo test` does not build the `cdylib` and `staticlib` of the package under test, so the
//! test asks cargo for them (`cargo build`, same profile and target directory; the lock is free
//! by then). Tests do not cross-compile: the C program must run on the machine.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;

/// The directory cargo puts this profile's artifacts in (`target/debug`), found from the test
/// executable (`target/debug/deps/c_roundtrip-HASH`).
fn profile_dir() -> PathBuf {
    let exe = env::current_exe().expect("path of the test executable");
    let dir = exe
        .parent()
        .and_then(Path::parent)
        .expect("the test executable is in target/PROFILE/deps");
    dir.to_path_buf()
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Builds the libraries once per test run (the two tests share them).
fn build_libraries() -> PathBuf {
    static BUILD: Once = Once::new();
    let dir = profile_dir();
    BUILD.call_once(|| {
        let profile = dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("profile name");
        let target_dir = dir.parent().expect("target directory");
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut command = Command::new(cargo);
        command
            .args(["build", "-p", "wsq-capi", "--target-dir"])
            .arg(target_dir)
            .arg("--manifest-path")
            .arg(crate_dir().join("Cargo.toml"));
        match profile {
            "debug" => {}
            "release" => {
                command.arg("--release");
            }
            custom => {
                command.args(["--profile", custom]);
            }
        }
        let status = command.status().expect("run cargo build");
        assert!(status.success(), "cargo build -p wsq-capi failed");
    });
    dir
}

/// The system libraries a program linking the static library must add, as `rustc` reports them
/// (`-lpthread -ldl ...`, or `kernel32.lib ...` for MSVC): compiling any staticlib shows them.
fn native_static_libs(dir: &Path) -> Vec<String> {
    let source = dir.join("c-tests").join("native_libs.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "#[unsafe(no_mangle)] pub extern \"C\" fn f() {}\n").unwrap();
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc)
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "staticlib",
            "--crate-name",
            "native_libs",
        ])
        .args(["--print", "native-static-libs", "--out-dir"])
        .arg(source.parent().unwrap())
        .arg(&source)
        .output()
        .expect("run rustc");
    assert!(
        output.status.success(),
        "rustc failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .lines()
        .find_map(|line| line.split_once("native-static-libs:").map(|(_, libs)| libs))
        .map(|libs| libs.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_else(|| panic!("no native-static-libs in the rustc output:\n{stderr}"))
}

fn host_triple() -> String {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc)
        .arg("-vV")
        .output()
        .expect("run rustc -vV");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("host triple in rustc -vV")
        .to_owned()
}

#[derive(Clone, Copy, PartialEq)]
enum Link {
    Shared,
    Static,
}

fn run_c_program(link: Link) {
    let dir = build_libraries();
    let work = dir.join("c-tests").join(if link == Link::Shared {
        "shared"
    } else {
        "static"
    });
    fs::create_dir_all(&work).unwrap();
    let include = crate_dir().join("include");
    let source = crate_dir().join("tests").join("roundtrip.c");
    let exe = work.join(if cfg!(windows) {
        "roundtrip.exe"
    } else {
        "roundtrip"
    });

    let host = host_triple();
    let tool = cc::Build::new()
        .cargo_metadata(false)
        .cargo_warnings(false)
        .target(&host)
        .host(&host)
        .opt_level(1)
        .debug(false)
        .try_get_compiler()
        .expect("find a C compiler");
    let sanitize = env::var_os("WSQ_C_SANITIZE").is_some_and(|v| !v.is_empty());

    let mut command = tool.to_command();
    command.current_dir(&work);
    if tool.is_like_msvc() {
        assert!(!sanitize, "WSQ_C_SANITIZE is for GCC and Clang");
        // /MD: the Rust libraries use the dynamic C runtime.
        command
            .args(["/nologo", "/W3", "/MD"])
            .arg(format!("/I{}", include.display()))
            .arg(&source)
            .arg(format!("/Fe:{}", exe.display()))
            .arg(format!("/Fo:{}", work.join("roundtrip.obj").display()));
        if link == Link::Shared {
            command.arg(dir.join("wsq.dll.lib"));
        } else {
            command.arg(dir.join("wsq.lib"));
            command.args(native_static_libs(&dir));
        }
    } else {
        command
            .args(["-std=c99", "-Wall", "-Wextra", "-g"])
            .arg("-I")
            .arg(&include);
        if sanitize {
            command.args(["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]);
        }
        command.arg(&source).arg("-o").arg(&exe);
        if link == Link::Shared {
            // The shared library is found by -l (the .so wins over the .a in the same
            // directory) and at run time through an rpath.
            let rpath = format!("-Wl,-rpath,{}", dir.display());
            command.arg("-L").arg(&dir).arg("-lwsq").arg(rpath);
        } else {
            command.arg(dir.join("libwsq.a"));
            command.args(native_static_libs(&dir));
        }
        command.arg("-lm");
    }
    let output = command.output().expect("run the C compiler");
    assert!(
        output.status.success(),
        "the C compiler failed ({:?}):\n{}\n{}",
        tool.path(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut run = Command::new(&exe);
    if link == Link::Shared && cfg!(windows) {
        // Windows looks for wsq.dll next to the program and then on PATH.
        let mut paths = vec![dir.clone()];
        paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
        run.env("PATH", env::join_paths(paths).unwrap());
    }
    let output = run.output().expect("run the C program");
    assert!(
        output.status.success(),
        "the C program failed ({:?}):\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("C API round trip ok"),
        "unexpected output: {stdout}"
    );
}

#[test]
fn c_program_with_the_shared_library() {
    run_c_program(Link::Shared);
}

#[test]
fn c_program_with_the_static_library() {
    run_c_program(Link::Static);
}

/// `include/wsq.h` must be usable from C++ too (it has `extern "C"` guards).
#[test]
fn header_compiles_as_cpp() {
    let Some(compiler) = ["c++", "g++", "clang++"].into_iter().find(|c| {
        Command::new(c)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }) else {
        eprintln!("no C++ compiler found, skipping");
        return;
    };
    if cfg!(windows) {
        return; // MSVC's cl takes other flags; the header is checked on Unix.
    }
    let work = profile_dir().join("c-tests");
    fs::create_dir_all(&work).unwrap();
    let source = work.join("header.cpp");
    fs::write(
        &source,
        "#include \"wsq.h\"\nint main() { return wsq_version() == nullptr; }\n",
    )
    .unwrap();
    let output = Command::new(compiler)
        .args(["-std=c++11", "-Wall", "-Wextra", "-fsyntax-only", "-I"])
        .arg(crate_dir().join("include"))
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
