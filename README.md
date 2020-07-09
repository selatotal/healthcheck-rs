# healthcheck-rs

A Rust executable for simple HTTP check status.

healthcheck-rs use a config file containing URLs and check if the return of HTTP GET request of each one is successfully

# Configure and build

Create a config.json file with the following specification:

```json
{
    "groups": [
        {
            "name": "GOOGLE SERVICES",
            "hosts": [
                {
                    "name": "Google Search", 
                    "url": "https://www.google.com"
                },
                {
                    "name": "Google Drive", 
                    "url": "https://drive.google.com"
                }
            ]
        },
        {
            "name": "FACEBOOK SERVICES",
            "hosts": [
                {
                    "name": "Facebook", 
                    "url": "https://facebook.com"
                },
                {
                    "name": "Instagram", 
                    "url": "https://instagram.com"
                }
            ]
        }
    ]
}
```

Install using:
```bash
cargo install healthcheck-rs
```

# Running
To run, you can use the binary generated in target/ folder or use (it will use config.json file in the same folder):
```bash
healthcheck-rs
```
You can pass config.json file in command-line using:
```bash
healthcheck-rs new-config-file.json
```

healthcheck-rs will do a HTTP GET request in each service and return an output like this:

```bash
===== GOOGLE SERVICES =====
Google Search - ✅
Google Drive - ✅
===== FACEBOOK SERVICES =====
Facebook - ❌ - reqwest::Error { kind: Request, url: "https://facebook.com", source: TimedOut }
Instagram - ✅
```

# Dependency Crates
This project use the following crates:
* [reqwest](https://crates.io/crates/reqwest) - An ergonomic, batteries-included HTTP Client for Ru
* [serde](https://crates.io/crates/serde) - Serde is a framework for serializing and deserializing Rust data structures efficiently and generically. 
* [serde-json](https://crates.io/crates/serde-json) - Serde is a framework for serializing and deserializing Rust data structures efficiently and generically in JSON format
