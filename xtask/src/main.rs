//! Build automation tasks for Bonnie Engine
//!
//! Usage:
//!   cargo xtask build-web        # Build WASM for web deployment
//!   cargo xtask build-web --dev  # Build with DEV banner

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build automation for Bonnie Engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build WASM for web deployment
    BuildWeb {
        /// Mark as dev build (adds DEV banner to index.html)
        #[arg(long)]
        dev: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::BuildWeb { dev } => build_web(dev),
    }
}

/// Get the project root directory
fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Run a command and check for success
fn run_cmd(cmd: &mut Command) -> Result<()> {
    let status = cmd.status().context("Failed to execute command")?;
    if !status.success() {
        anyhow::bail!("Command failed with status: {}", status);
    }
    Ok(())
}

/// Download a file from URL to destination
fn download_file(url: &str, dest: &Path) -> Result<()> {
    println!("Downloading {}...", url);
    run_cmd(
        Command::new("curl")
            .args(["-L", "-o"])
            .arg(dest)
            .arg(url),
    )
}

/// Directories to exclude from web builds (reduces file count for itch.io)
const EXCLUDED_ASSET_DIRS: &[&str] = &[
    "quake-like",
    "dark-fantasy-townhouse",
];

/// Copy directory recursively, skipping excluded directory names
fn copy_dir_recursive_filtered(src: &Path, dst: &Path, exclude: &[&str]) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip excluded directories
        if src_path.is_dir() && exclude.iter().any(|e| *e == name_str) {
            println!("Skipping excluded directory: {}", src_path.display());
            continue;
        }

        if src_path.is_dir() {
            copy_dir_recursive_filtered(&src_path, &dst_path, exclude)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Build WASM for web deployment
fn build_web(dev: bool) -> Result<()> {
    let root = project_root();
    let dist = root.join("dist/web");

    println!("Building WASM...");
    run_cmd(
        Command::new("cargo")
            .current_dir(&root)
            .args(["build", "--release", "--target", "wasm32-unknown-unknown"]),
    )?;

    // Clean and create dist folder
    if dist.exists() {
        std::fs::remove_dir_all(&dist)?;
    }
    std::fs::create_dir_all(&dist)?;

    // Copy WASM binary
    println!("Copying files to dist/web...");
    std::fs::copy(
        root.join("target/wasm32-unknown-unknown/release/bonnie-engine.wasm"),
        dist.join("bonnie-engine.wasm"),
    )?;

    // Copy web files from docs/
    let docs = root.join("docs");
    for file in ["index.html", "audio-processor.js", "favicon-16.png", "favicon-32.png", "apple-touch-icon.png"] {
        let src = docs.join(file);
        if src.exists() {
            std::fs::copy(&src, dist.join(file))?;
        }
    }

    // Download macroquad JS bundle
    let mq_js = dist.join("mq_js_bundle.js");
    if !mq_js.exists() {
        download_file(
            "https://raw.githubusercontent.com/not-fl3/macroquad/v0.4.14/js/mq_js_bundle.js",
            &mq_js,
        )?;
    }

    // Copy assets (excluding large/unused directories to stay under itch.io file limit)
    copy_dir_recursive_filtered(&root.join("assets"), &dist.join("assets"), EXCLUDED_ASSET_DIRS)?;

    // Apply dev modifications if requested
    if dev {
        println!("Applying DEV build modifications...");
        let index_path = dist.join("index.html");
        let index = std::fs::read_to_string(&index_path)?;
        let index = index
            .replace("Loading Bonnie Engine", "Loading Bonnie Engine (DEV)")
            .replace("<title>Bonnie Engine", "<title>[DEV] Bonnie Engine");
        std::fs::write(&index_path, index)?;
    }

    println!("Web build complete: dist/web/");
    Ok(())
}
