use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::ErrorKind::NotFound;
use std::time::Duration;
use serde::Deserialize;

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

fn main() {
    
    // Check if config file was passed in command line
    let args: Vec<String> = env::args().collect();
 
    // Reading config file
    let file;
    if args.len() > 1 {
        file = File::open(&args[1]);
    } else {
        file = File::open("config.json");
    }

    if let Err(e) = file {
        if e.kind() == NotFound {
            println!("config.json file not found!");
        } else {
            println!("Error reading config file: {}", e.to_string());
        }
        println!("Please provide a valid config file.");
        println!("You can check the documentation at https://github.com/selatotal/healthcheck-rs");
        return;
    }

    let mut contents = String::new();
    if let Err(e) = file.unwrap().read_to_string(&mut contents){
        println!("Invalid config file: {}", e.to_string());
        println!("Please provide a valid config file.");
        println!("You can check the documentation at https://github.com/selatotal/healthcheck-rs");
        return;        
    }

    let config: Config = match serde_json::from_str(&contents) {
        Ok(config) => config,
        Err(e) => {
            println!("Invalid config file: {}", e.to_string());
            println!("Please provide a valid config file.");
            println!("You can check the documentation at https://github.com/selatotal/healthcheck-rs");
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
            match client.get(&host_config.url).send() {
                Ok(object) => {
                    if object.status().is_success() {
                        println!("{} - {}", host_config.name, '\u{2705}');
                    } else {
                        println!("{} - {} - {}", host_config.name, '\u{274c}', object.status().as_str());
                    }
                },
                Err(e) => {
                    println!("{} - {} - {:?}", host_config.name, '\u{274c}', e);
                },
            }
        }    
    }
}
