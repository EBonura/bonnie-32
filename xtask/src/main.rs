//! Build automation tasks for Bonnie Engine
//!
//! Usage:
//!   cargo xtask build-web       # Build WASM for web deployment
//!   cargo xtask package-itch    # Create zip for itch.io upload
//!   cargo xtask package-steam   # Build native for Steam (placeholder)

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
    /// Build WASM for web deployment (GitHub Pages)
    BuildWeb {
        /// Mark as dev build (adds DEV banner to index.html)
        #[arg(long)]
        dev: bool,
    },
    /// Create zip file ready for itch.io upload
    PackageItch,
    /// Build native executables for Steam (placeholder)
    PackageSteam {
        /// Target platform: windows, macos, linux
        #[arg(long)]
        platform: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::BuildWeb { dev } => build_web(dev),
        Commands::PackageItch => package_itch(),
        Commands::PackageSteam { platform } => package_steam(platform),
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

/// Copy directory recursively
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
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

    // Copy assets
    copy_dir_recursive(&root.join("assets"), &dist.join("assets"))?;

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

/// Create zip for itch.io
fn package_itch() -> Result<()> {
    // First build web
    build_web(false)?;

    let root = project_root();
    let dist = root.join("dist");
    let zip_path = dist.join("bonnie-engine-itch.zip");

    // Remove old zip if exists
    if zip_path.exists() {
        std::fs::remove_file(&zip_path)?;
    }

    println!("Creating itch.io zip...");
    run_cmd(
        Command::new("zip")
            .current_dir(dist.join("web"))
            .args(["-r", "../bonnie-engine-itch.zip", "."]),
    )?;

    println!("itch.io package ready: dist/bonnie-engine-itch.zip");
    Ok(())
}

/// Build for Steam (placeholder)
fn package_steam(platform: Option<String>) -> Result<()> {
    let root = project_root();
    let platform = platform.unwrap_or_else(|| {
        if cfg!(target_os = "windows") {
            "windows".to_string()
        } else if cfg!(target_os = "macos") {
            "macos".to_string()
        } else {
            "linux".to_string()
        }
    });

    let dist = root.join(format!("dist/steam/{}", platform));

    println!("Building native release for {}...", platform);

    // Clean and create dist folder
    if dist.exists() {
        std::fs::remove_dir_all(&dist)?;
    }
    std::fs::create_dir_all(&dist)?;

    // Build native release
    run_cmd(
        Command::new("cargo")
            .current_dir(&root)
            .args(["build", "--release"]),
    )?;

    // Copy binary
    let binary_name = if platform == "windows" {
        "bonnie-engine.exe"
    } else {
        "bonnie-engine"
    };

    std::fs::copy(
        root.join(format!("target/release/{}", binary_name)),
        dist.join(binary_name),
    )?;

    // Copy assets
    copy_dir_recursive(&root.join("assets"), &dist.join("assets"))?;

    println!("Steam build complete: dist/steam/{}/", platform);
    println!("Note: Steamworks SDK integration not yet implemented");

    Ok(())
}
