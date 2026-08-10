use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::ErrorKind::NotFound;
use std::path::Path;
use std::time::Duration;
use serde::Deserialize;

const DEFAULT_CONFIG_FILE: &str = "healthcheck-rs.json";
const LEGACY_CONFIG_FILE: &str = "config.json";
const DOCS_URL: &str = "https://github.com/selatotal/healthcheck-rs";

#[derive(Deserialize, Debug)]
struct Config {
    pub groups: Vec<HostGroup>,
}

#[derive(Deserialize, Debug)]
struct HostGroup {
    pub name: String,
    pub hosts: Vec<HostConfig>,
}

#[derive(Deserialize, Debug)]
struct HostConfig {
    pub name: String,
    pub url: String,
}

/// Where the checks should be read from, given the command line and which of
/// the well-known config files exist in the current folder.
#[derive(Debug, PartialEq)]
enum ConfigSource {
    /// Path passed explicitly on the command line.
    Explicit(String),
    /// Default `healthcheck-rs.json`.
    Default,
    /// Deprecated `config.json`, kept as a fallback for configs written before
    /// the file was renamed.
    Legacy,
    /// Neither well-known file exists.
    Missing,
}

/// Result of checking a single host, kept separate from the HTTP client so the
/// report formatting can be tested without network access.
enum CheckOutcome {
    Up,
    Down(String),
}

fn main() {

    // Check if config file was passed in command line
    let args: Vec<String> = env::args().collect();

    let path = match resolve_config_source(&args[1..], |file| Path::new(file).exists()) {
        ConfigSource::Explicit(path) => path,
        ConfigSource::Default => DEFAULT_CONFIG_FILE.to_string(),
        ConfigSource::Legacy => {
            eprintln!("Warning: {} is deprecated, rename it to {}.", LEGACY_CONFIG_FILE, DEFAULT_CONFIG_FILE);
            LEGACY_CONFIG_FILE.to_string()
        },
        ConfigSource::Missing => {
            println!("{} file not found!", DEFAULT_CONFIG_FILE);
            print_config_help();
            return;
        },
    };

    // Reading config file
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(e) => {
            if e.kind() == NotFound {
                println!("{} file not found!", path);
            } else {
                println!("Error reading config file: {}", e);
            }
            print_config_help();
            return;
        },
    };

    let mut contents = String::new();
    if let Err(e) = file.read_to_string(&mut contents) {
        println!("Invalid config file: {}", e);
        print_config_help();
        return;
    }

    let config: Config = match serde_json::from_str(&contents) {
        Ok(config) => config,
        Err(e) => {
            println!("Invalid config file: {}", e);
            print_config_help();
            return;
        },
    };

    // Doing requests
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    for group in config.groups {
        println!("===== {} =====", group.name);
        for host_config in group.hosts {
            let outcome = match client.get(&host_config.url).send() {
                Ok(response) => {
                    if response.status().is_success() {
                        CheckOutcome::Up
                    } else {
                        CheckOutcome::Down(response.status().as_str().to_string())
                    }
                },
                Err(e) => CheckOutcome::Down(format!("{:?}", e)),
            };
            println!("{}", format_check(&host_config.name, &outcome));
        }
    }
}

/// An explicit path always wins; otherwise the default file is preferred over
/// the deprecated one. `exists` reports whether a well-known file is present,
/// so this stays independent of the filesystem.
fn resolve_config_source(args: &[String], exists: impl Fn(&str) -> bool) -> ConfigSource {
    if let Some(path) = args.first() {
        return ConfigSource::Explicit(path.clone());
    }
    if exists(DEFAULT_CONFIG_FILE) {
        ConfigSource::Default
    } else if exists(LEGACY_CONFIG_FILE) {
        ConfigSource::Legacy
    } else {
        ConfigSource::Missing
    }
}

fn format_check(name: &str, outcome: &CheckOutcome) -> String {
    match outcome {
        CheckOutcome::Up => format!("{} - {}", name, '\u{2705}'),
        CheckOutcome::Down(detail) => format!("{} - {} - {}", name, '\u{274c}', detail),
    }
}

fn print_config_help() {
    println!("Please provide a valid config file.");
    println!("You can check the documentation at {}", DOCS_URL);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn explicit_argument_wins_over_existing_files() {
        let source = resolve_config_source(&args(&["custom.json"]), |_| true);
        assert_eq!(source, ConfigSource::Explicit("custom.json".to_string()));
    }

    #[test]
    fn explicit_argument_is_used_even_when_it_does_not_exist() {
        let source = resolve_config_source(&args(&["missing.json"]), |_| false);
        assert_eq!(source, ConfigSource::Explicit("missing.json".to_string()));
    }

    #[test]
    fn default_file_is_preferred_over_legacy() {
        let source = resolve_config_source(&[], |_| true);
        assert_eq!(source, ConfigSource::Default);
    }

    #[test]
    fn legacy_file_is_used_when_default_is_absent() {
        let source = resolve_config_source(&[], |file| file == LEGACY_CONFIG_FILE);
        assert_eq!(source, ConfigSource::Legacy);
    }

    #[test]
    fn missing_when_no_well_known_file_exists() {
        let source = resolve_config_source(&[], |_| false);
        assert_eq!(source, ConfigSource::Missing);
    }

    #[test]
    fn up_host_is_reported_with_a_check_mark() {
        assert_eq!(format_check("Google", &CheckOutcome::Up), "Google - \u{2705}");
    }

    #[test]
    fn down_host_is_reported_with_the_failure_detail() {
        let outcome = CheckOutcome::Down("500".to_string());
        assert_eq!(format_check("Google", &outcome), "Google - \u{274c} - 500");
    }

    #[test]
    fn config_is_parsed_into_groups_and_hosts() {
        let contents = r#"{
            "groups": [
                {
                    "name": "GOOGLE SERVICES",
                    "hosts": [
                        { "name": "Google Search", "url": "https://www.google.com" }
                    ]
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(contents).expect("valid config");

        assert_eq!(config.groups.len(), 1);
        assert_eq!(config.groups[0].name, "GOOGLE SERVICES");
        assert_eq!(config.groups[0].hosts.len(), 1);
        assert_eq!(config.groups[0].hosts[0].name, "Google Search");
        assert_eq!(config.groups[0].hosts[0].url, "https://www.google.com");
    }

    #[test]
    fn config_without_groups_is_rejected() {
        assert!(serde_json::from_str::<Config>("{}").is_err());
    }

    #[test]
    fn host_without_url_is_rejected() {
        let contents = r#"{"groups": [{"name": "G", "hosts": [{"name": "no url"}]}]}"#;
        assert!(serde_json::from_str::<Config>(contents).is_err());
    }
}
