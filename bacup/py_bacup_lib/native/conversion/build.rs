#[cfg(windows)]
fn main() {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    let interpreter = pyo3_build_config::get()
        .executable
        .as_ref()
        .expect("PyO3 did not resolve a Python interpreter");
    println!("cargo:rerun-if-changed={interpreter}");
    let output = Command::new(interpreter)
        .args([
            "-c",
            "import sys; print(sys.base_prefix); print(f'python{sys.version_info.major}{sys.version_info.minor}.dll')",
        ])
        .output()
        .expect("failed to query the PyO3 Python interpreter");
    assert!(
        output.status.success(),
        "failed to query the PyO3 Python interpreter: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("Python paths were not UTF-8");
    let mut lines = stdout.lines();
    let python_home = PathBuf::from(lines.next().expect("Python did not report sys.base_prefix"));
    let version_dll = lines.next().expect("Python did not report its runtime DLL");
    println!(
        "cargo:rustc-env=PYO3_TEST_PYTHON_HOME={}",
        python_home.display()
    );

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR was not set"));
    let deps_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR did not contain a Cargo profile directory")
        .join("deps");
    fs::create_dir_all(&deps_dir).expect("failed to create Cargo deps directory");

    for dll in ["python3.dll", version_dll] {
        let source = python_home.join(dll);
        println!("cargo:rerun-if-changed={}", source.display());
        copy_if_changed(&source, &deps_dir.join(dll));
    }

    fn copy_if_changed(source: &Path, destination: &Path) {
        let source_bytes = fs::read(source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        if fs::read(destination).is_ok_and(|current| current == source_bytes) {
            return;
        }
        fs::write(destination, source_bytes)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", destination.display()));
    }
}

#[cfg(not(windows))]
fn main() {}
