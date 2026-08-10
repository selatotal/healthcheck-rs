use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_healthcheck-rs");

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

/// A throwaway working directory, so each test controls exactly which of the
/// well-known config files exist while the binary runs.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new() -> Sandbox {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("healthcheck-rs-it-{}-{}", std::process::id(), id));
        fs::create_dir_all(&dir).expect("create sandbox dir");
        Sandbox { dir }
    }

    /// Groups with no hosts keep these tests offline: the binary still prints a
    /// header naming the file it read, but never issues a request.
    fn write_config(&self, name: &str, group: &str) {
        let contents = format!("{{\"groups\": [{{\"name\": \"{}\", \"hosts\": []}}]}}", group);
        fs::write(self.dir.join(name), contents).expect("write config");
    }

    fn write_raw(&self, name: &str, contents: &str) {
        fs::write(self.dir.join(name), contents).expect("write file");
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("run healthcheck-rs")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn reads_the_default_config_file() {
    let sandbox = Sandbox::new();
    sandbox.write_config("healthcheck-rs.json", "FROM DEFAULT");

    let output = sandbox.run(&[]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("===== FROM DEFAULT ====="), "{}", stdout(&output));
    assert!(!stderr(&output).contains("deprecated"), "{}", stderr(&output));
}

#[test]
fn falls_back_to_the_legacy_config_file_with_a_warning() {
    let sandbox = Sandbox::new();
    sandbox.write_config("config.json", "FROM LEGACY");

    let output = sandbox.run(&[]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("===== FROM LEGACY ====="), "{}", stdout(&output));
    assert!(stderr(&output).contains("config.json is deprecated"), "{}", stderr(&output));
    assert!(stderr(&output).contains("healthcheck-rs.json"), "{}", stderr(&output));
}

#[test]
fn prefers_the_default_config_file_over_the_legacy_one() {
    let sandbox = Sandbox::new();
    sandbox.write_config("healthcheck-rs.json", "FROM DEFAULT");
    sandbox.write_config("config.json", "FROM LEGACY");

    let output = sandbox.run(&[]);

    assert!(stdout(&output).contains("===== FROM DEFAULT ====="), "{}", stdout(&output));
    assert!(!stdout(&output).contains("FROM LEGACY"), "{}", stdout(&output));
    assert!(!stderr(&output).contains("deprecated"), "{}", stderr(&output));
}

#[test]
fn command_line_path_overrides_both_well_known_files() {
    let sandbox = Sandbox::new();
    sandbox.write_config("healthcheck-rs.json", "FROM DEFAULT");
    sandbox.write_config("config.json", "FROM LEGACY");
    sandbox.write_config("other.json", "FROM ARGUMENT");

    let output = sandbox.run(&["other.json"]);

    assert!(stdout(&output).contains("===== FROM ARGUMENT ====="), "{}", stdout(&output));
    assert!(!stderr(&output).contains("deprecated"), "{}", stderr(&output));
}

#[test]
fn missing_config_reports_the_current_filename() {
    let sandbox = Sandbox::new();

    let output = sandbox.run(&[]);

    // Failures are reported, not propagated: the process still exits 0.
    assert!(output.status.success());
    assert!(stdout(&output).contains("healthcheck-rs.json file not found!"), "{}", stdout(&output));
    assert!(stdout(&output).contains("Please provide a valid config file."), "{}", stdout(&output));
}

#[test]
fn missing_explicit_path_is_reported_by_name() {
    let sandbox = Sandbox::new();
    sandbox.write_config("healthcheck-rs.json", "FROM DEFAULT");

    let output = sandbox.run(&["nope.json"]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("nope.json file not found!"), "{}", stdout(&output));
    assert!(!stdout(&output).contains("FROM DEFAULT"), "{}", stdout(&output));
}

#[test]
fn invalid_json_is_reported() {
    let sandbox = Sandbox::new();
    sandbox.write_raw("healthcheck-rs.json", "{ not json");

    let output = sandbox.run(&[]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("Invalid config file:"), "{}", stdout(&output));
}
