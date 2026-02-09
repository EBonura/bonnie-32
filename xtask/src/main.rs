//! Build automation tasks for BONNIE-32
//!
//! Usage:
//!   cargo xtask build-web        # Build WASM for web deployment
//!   cargo xtask build-web --dev  # Build with DEV banner
//!   cargo xtask serve            # Build and serve locally on port 8080
//!   cargo xtask serve -p 3000    # Build and serve on custom port

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build automation for BONNIE-32")]
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
    /// Build and serve locally for testing
    Serve {
        /// Port to serve on (default: 8080)
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::BuildWeb { dev } => build_web(dev),
        Commands::Serve { port } => serve(port),
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
        root.join("target/wasm32-unknown-unknown/release/bonnie-32.wasm"),
        dist.join("bonnie-32.wasm"),
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

    // Regenerate texture-pack manifest without excluded packs
    regenerate_texture_manifest(&dist.join("assets/samples/texture-packs"))?;

    // Generate manifest for sample CLUT textures (RON files)
    regenerate_user_texture_manifest(&dist.join("assets/samples/textures"))?;

    // Generate manifest for user textures (for WASM loading)
    regenerate_user_texture_manifest(&dist.join("assets/userdata/textures"))?;

    // Apply dev modifications if requested
    if dev {
        println!("Applying DEV build modifications...");
        let index_path = dist.join("index.html");
        let index = std::fs::read_to_string(&index_path)?;
        let index = index
            .replace("Loading BONNIE-32", "Loading BONNIE-32 (DEV)")
            .replace("<title>BONNIE-32", "<title>[DEV] BONNIE-32");
        std::fs::write(&index_path, index)?;
    }

    println!("Web build complete: dist/web/");
    Ok(())
}

/// Build and serve locally for testing
fn serve(port: u16) -> Result<()> {
    let root = project_root();
    let dist = root.join("dist/web");

    // Build first
    build_web(false)?;

    // Kill any existing server on this port (best effort)
    #[cfg(unix)]
    {
        let _ = Command::new("sh")
            .args(["-c", &format!("lsof -ti:{} | xargs kill -9 2>/dev/null", port)])
            .status();
    }

    println!("\n🚀 Starting local server at http://localhost:{}", port);
    println!("   Press Ctrl+C to stop\n");

    // Start Python HTTP server
    run_cmd(
        Command::new("python3")
            .current_dir(&dist)
            .args(["-m", "http.server", &port.to_string()]),
    )?;

    Ok(())
}

/// Regenerate texture manifest based on actual directories present
fn regenerate_texture_manifest(textures_dir: &Path) -> Result<()> {
    let mut manifest = String::new();

    // Get sorted list of texture pack directories
    let mut packs: Vec<_> = std::fs::read_dir(textures_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    packs.sort_by_key(|e| e.file_name());

    for pack in packs {
        let pack_name = pack.file_name();
        let pack_name = pack_name.to_string_lossy();

        manifest.push_str(&format!("[{}]\n", pack_name));

        // Get sorted list of textures in this pack
        let mut textures: Vec<_> = std::fs::read_dir(pack.path())?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "png" || ext == "jpg" || ext == "jpeg")
                    .unwrap_or(false)
            })
            .collect();
        textures.sort_by_key(|e| e.file_name());

        for tex in textures {
            manifest.push_str(&format!("{}\n", tex.file_name().to_string_lossy()));
        }
    }

    std::fs::write(textures_dir.join("manifest.txt"), manifest)?;
    println!("Regenerated texture manifest");
    Ok(())
}

/// Generate manifest for user textures (flat list of .ron files)
fn regenerate_user_texture_manifest(textures_user_dir: &Path) -> Result<()> {
    // If directory doesn't exist, skip
    if !textures_user_dir.exists() {
        println!("No textures-user directory, skipping manifest generation");
        return Ok(());
    }

    let mut manifest = String::new();

    // Get sorted list of .ron files
    let mut files: Vec<_> = std::fs::read_dir(textures_user_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "ron")
                .unwrap_or(false)
        })
        .collect();
    files.sort_by_key(|e| e.file_name());

    // Write one filename per line
    for file in files {
        manifest.push_str(&format!("{}\n", file.file_name().to_string_lossy()));
    }

    let file_count = manifest.lines().count();
    std::fs::write(textures_user_dir.join("manifest.txt"), manifest)?;
    println!("Regenerated user texture manifest ({} files)", file_count);
    Ok(())
}
