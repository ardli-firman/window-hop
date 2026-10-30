//! Version management helper for Window Hop (Pure Rust).
//! Requires only rustc/cargo from Rust toolchain (no external crates).
//!
//! Compilation:
//!   rustc -O scripts/version.rs -o target/version-tool
//!
//! Usage:
//!   target/version-tool get
//!   target/version-tool full
//!   target/version-tool set <VERSION>
//!   target/version-tool bump <patch|minor|major>

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_cargo_toml() -> PathBuf {
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest_dir).join("Cargo.toml");
        if p.exists() {
            return p;
        }
    }
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let direct = current_dir.join("Cargo.toml");
    if direct.exists() {
        return direct;
    }
    let parent = current_dir.join("..").join("Cargo.toml");
    if parent.exists() {
        return parent;
    }
    PathBuf::from("Cargo.toml")
}

fn get_version(cargo_toml_content: &str) -> Result<String, String> {
    let mut in_package = false;
    for line in cargo_toml_content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && trimmed.starts_with('[') {
            break;
        }
        if in_package && trimmed.starts_with("version") {
            if let Some((_, val)) = trimmed.split_once('=') {
                let v = val.trim().trim_matches('"');
                return Ok(v.to_string());
            }
        }
    }
    Err("Could not find [package] version in Cargo.toml".to_string())
}

fn get_git_describe() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn validate_semver(version: &str) -> Result<(u64, u64, u64), String> {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(format!(
            "Invalid SemVer '{}': expected 3 numeric segments (major.minor.patch)",
            version
        ));
    }
    let major = parts[0]
        .parse::<u64>()
        .map_err(|_| format!("Invalid major version '{}'", parts[0]))?;
    let minor = parts[1]
        .parse::<u64>()
        .map_err(|_| format!("Invalid minor version '{}'", parts[1]))?;
    let patch = parts[2]
        .parse::<u64>()
        .map_err(|_| format!("Invalid patch version '{}'", parts[2]))?;
    Ok((major, minor, patch))
}

fn set_version(cargo_toml_path: &Path, new_version: &str) -> Result<(), String> {
    validate_semver(new_version)?;
    let content = fs::read_to_string(cargo_toml_path)
        .map_err(|e| format!("Failed to read Cargo.toml: {}", e))?;

    let mut in_package = false;
    let mut replaced = false;
    let mut new_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            new_lines.push(line.to_string());
            continue;
        }
        if in_package && trimmed.starts_with('[') {
            in_package = false;
        }

        if in_package && !replaced && trimmed.starts_with("version") {
            new_lines.push(format!("version = \"{}\"", new_version));
            replaced = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if !replaced {
        return Err("Could not locate [package] version line in Cargo.toml".to_string());
    }

    let trimmed_output = new_lines.join("\n").trim_end().to_string() + "\n";
    fs::write(cargo_toml_path, trimmed_output)
        .map_err(|e| format!("Failed to write Cargo.toml: {}", e))?;

    // Update Cargo.lock if cargo is present
    let _ = Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .status();

    Ok(())
}

fn bump_version(cargo_toml_path: &Path, part: &str) -> Result<String, String> {
    let content = fs::read_to_string(cargo_toml_path)
        .map_err(|e| format!("Failed to read Cargo.toml: {}", e))?;
    let current = get_version(&content)?;
    let (mut major, mut minor, mut patch) = validate_semver(&current)?;

    match part.to_lowercase().as_str() {
        "patch" => patch += 1,
        "minor" => {
            minor += 1;
            patch = 0;
        }
        "major" => {
            major += 1;
            minor = 0;
            patch = 0;
        }
        other => {
            return Err(format!(
                "Unknown bump target '{}'. Choose from: patch, minor, major",
                other
            ))
        }
    }

    let new_version = format!("{}.{}.{}", major, minor, patch);
    set_version(cargo_toml_path, &new_version)?;
    Ok(new_version)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: version-tool <get|full|set <VERSION>|bump <patch|minor|major>>");
        std::process::exit(1);
    }

    let cargo_toml_path = find_cargo_toml();
    let cmd = args[1].to_lowercase();

    match cmd.as_str() {
        "get" => {
            let content = match fs::read_to_string(&cargo_toml_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading {}: {}", cargo_toml_path.display(), e);
                    std::process::exit(1);
                }
            };
            match get_version(&content) {
                Ok(v) => println!("{}", v),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "full" => {
            let content = match fs::read_to_string(&cargo_toml_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading {}: {}", cargo_toml_path.display(), e);
                    std::process::exit(1);
                }
            };
            let ver = match get_version(&content) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            match get_git_describe() {
                Some(info) if !info.is_empty() => println!("{} ({})", ver, info),
                _ => println!("{}", ver),
            }
        }
        "set" => {
            if args.len() < 3 {
                eprintln!("Error: Missing version argument. Example: version-tool set 0.2.0");
                std::process::exit(1);
            }
            let new_ver = &args[2];
            if let Err(e) = set_version(&cargo_toml_path, new_ver) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            println!("Updated version to {}", new_ver);
        }
        "bump" => {
            if args.len() < 3 {
                eprintln!("Error: Missing bump target. Example: version-tool bump patch");
                std::process::exit(1);
            }
            let part = &args[2];
            match bump_version(&cargo_toml_path, part) {
                Ok(new_ver) => println!("Bumped {} version to {}", part, new_ver),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("Unknown command: {}", other);
            eprintln!("Usage: version-tool <get|full|set <VERSION>|bump <patch|minor|major>>");
            std::process::exit(1);
        }
    }
}
