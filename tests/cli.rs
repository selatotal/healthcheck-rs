use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

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

    fn write_hosts(&self, name: &str, group: &str, hosts: &[(&str, String)]) {
        let hosts: Vec<String> = hosts
            .iter()
            .map(|(host, url)| format!("{{\"name\": \"{}\", \"url\": \"{}\"}}", host, url))
            .collect();
        let contents = format!("{{\"groups\": [{{\"name\": \"{}\", \"hosts\": [{}]}}]}}", group, hosts.join(", "));
        fs::write(self.dir.join(name), contents).expect("write config");
    }

    /// Proxy variables are cleared so requests to the loopback test servers
    /// never leave the machine.
    fn run(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .args(args)
            .current_dir(&self.dir)
            .env_remove("HTTP_PROXY")
            .env_remove("http_proxy")
            .env_remove("HTTPS_PROXY")
            .env_remove("https_proxy")
            .env_remove("ALL_PROXY")
            .env_remove("all_proxy")
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env("no_proxy", "127.0.0.1,localhost")
            .output()
            .expect("run healthcheck-rs")
    }
}

/// Starts a loopback HTTP server that answers every request with `status` and
/// returns its base URL. The accept thread lives until the test process exits.
fn serve(status: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let url = format!("http://{}/", listener.local_addr().expect("server addr"));
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                }
            }
            let response = format!("HTTP/1.1 {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", status);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

/// A loopback URL nothing listens on, so the connection is refused.
fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind probe");
    let addr = listener.local_addr().expect("probe addr");
    drop(listener);
    format!("http://{}/", addr)
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

#[test]
fn config_path_that_is_a_directory_is_reported_as_invalid() {
    let sandbox = Sandbox::new();
    fs::create_dir(sandbox.dir.join("healthcheck-rs.json")).expect("create dir");

    let output = sandbox.run(&[]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("Invalid config file:"), "{}", stdout(&output));
    assert!(stdout(&output).contains("Please provide a valid config file."), "{}", stdout(&output));
}

#[cfg(unix)]
#[test]
fn unreadable_config_is_reported_as_a_read_error() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = Sandbox::new();
    sandbox.write_config("healthcheck-rs.json", "UNREADABLE");
    let path = sandbox.dir.join("healthcheck-rs.json");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod");
    if fs::File::open(&path).is_ok() {
        // Running as root ignores file permissions, so the error path cannot be reached.
        return;
    }

    let output = sandbox.run(&[]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("Error reading config file:"), "{}", stdout(&output));
    assert!(!stdout(&output).contains("UNREADABLE"), "{}", stdout(&output));
}

#[test]
fn healthy_host_is_reported_as_up() {
    let sandbox = Sandbox::new();
    sandbox.write_hosts("healthcheck-rs.json", "LOCAL", &[("Healthy", serve("200 OK"))]);

    let output = sandbox.run(&[]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("===== LOCAL ====="), "{}", stdout(&output));
    assert!(stdout(&output).contains("Healthy - \u{2705}"), "{}", stdout(&output));
}

#[test]
fn failing_host_is_reported_with_its_status_code_and_exit_stays_zero() {
    let sandbox = Sandbox::new();
    sandbox.write_hosts("healthcheck-rs.json", "LOCAL", &[("Broken", serve("500 Internal Server Error"))]);

    let output = sandbox.run(&[]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("Broken - \u{274c} - 500"), "{}", stdout(&output));
}

#[test]
fn unreachable_host_is_reported_with_the_request_error() {
    let sandbox = Sandbox::new();
    sandbox.write_hosts("healthcheck-rs.json", "LOCAL", &[("Offline", closed_port_url())]);

    let output = sandbox.run(&[]);

    assert!(output.status.success());
    assert!(stdout(&output).contains("Offline - \u{274c} - reqwest::Error"), "{}", stdout(&output));
}

#[test]
fn every_host_in_every_group_is_checked_in_order() {
    let sandbox = Sandbox::new();
    let up = serve("204 No Content");
    let down = serve("404 Not Found");
    let contents = format!(
        "{{\"groups\": [\
            {{\"name\": \"FIRST\", \"hosts\": [{{\"name\": \"A\", \"url\": \"{}\"}}, {{\"name\": \"B\", \"url\": \"{}\"}}]}},\
            {{\"name\": \"SECOND\", \"hosts\": [{{\"name\": \"C\", \"url\": \"{}\"}}]}}\
        ]}}",
        up, down, up
    );
    sandbox.write_raw("healthcheck-rs.json", &contents);

    let output = sandbox.run(&[]);
    let out = stdout(&output);

    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        vec!["===== FIRST =====", "A - \u{2705}", "B - \u{274c} - 404", "===== SECOND =====", "C - \u{2705}"],
        "{}",
        out
    );
}
