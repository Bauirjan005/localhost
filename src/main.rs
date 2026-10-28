use std::env;
use std::process;

use localhost::config::{ConfigParser, ConfigValidator};
use localhost::server::Server;

fn main() {
    let args: Vec<String> = env::args().collect();
    let config_path = if args.len() > 1 {
        &args[1]
    } else {
        "config/default.conf"
    };

    println!("============================================================");
    println!("     Localhost HTTP/1.1 Server (Rust Event-Driven I/O)      ");
    println!("============================================================");
    println!("Loading configuration from: '{}'...", config_path);

    let config = match ConfigParser::parse_file(config_path) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Configuration Error: {}", err);
            process::exit(1);
        }
    };

    println!("Validating configuration...");
    if let Err(errors) = ConfigValidator::validate(&config) {
        eprintln!("Configuration Validation Failed with {} error(s):", errors.len());
        for (i, err) in errors.iter().enumerate() {
            eprintln!("  [{}] {}", i + 1, err);
        }
        process::exit(1);
    }
    println!("Configuration validated successfully ({} server block(s) configured).", config.servers.len());

    let mut server = match Server::new(config) {
        Ok(srv) => srv,
        Err(e) => {
            eprintln!("Failed to initialize server: {}", e);
            process::exit(1);
        }
    };

    if let Err(e) = server.start() {
        eprintln!("Server error: {}", e);
        process::exit(1);
    }
}
