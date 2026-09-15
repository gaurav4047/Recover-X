//! RecoverX CLI
//!
//! Command-line interface to the RecoverX recovery engine.
//! Shares the same backend as the Tauri GUI — no duplicate logic.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use recoverx_core::{
    identity::SourceIdentity,
    types::{ScanConfiguration, SessionId},
};
use recoverx_devices::platform_provider;
use recoverx_logging::{init, LogConfig};
use recoverx_orchestrator::RecoveryOrchestrator;

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(
    name = "recoverx",
    about = "RecoverX — Advanced Data Recovery & Digital Forensics",
    version = env!("CARGO_PKG_VERSION"),
    long_about = None,
)]
struct Cli {
    /// Enable verbose debug logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Emit JSON-formatted log output
    #[arg(long, global = true)]
    json_log: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List detected storage devices
    Devices,

    /// Manage scan sessions
    Sessions {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Start a scan
    Scan {
        /// Device path or image file to scan
        source: String,

        /// Scan mode: quick | deep | forensic | carving
        #[arg(short, long, default_value = "quick")]
        mode: String,

        /// Data directory for session persistence
        #[arg(long, default_value = "./recoverx-data")]
        data_dir: PathBuf,

        /// Size of source in bytes (required for image files)
        #[arg(long)]
        size: Option<u64>,
    },
}

#[derive(Debug, Subcommand)]
enum SessionAction {
    /// List all sessions
    List {
        #[arg(long, default_value = "./recoverx-data")]
        data_dir: PathBuf,
    },
    /// Show session details
    Show {
        session_id: String,
        #[arg(long, default_value = "./recoverx-data")]
        data_dir: PathBuf,
    },
    /// Delete a session
    Delete {
        session_id: String,
        #[arg(long, default_value = "./recoverx-data")]
        data_dir: PathBuf,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let log_level = if cli.verbose { "debug" } else { "info" };
    init(LogConfig {
        level: log_level.to_string(),
        json: cli.json_log,
        log_file: None,
    })?;

    match cli.command {
        Commands::Devices => cmd_devices().await,
        Commands::Sessions { action } => cmd_sessions(action).await,
        Commands::Scan {
            source,
            mode,
            data_dir,
            size,
        } => cmd_scan(source, mode, data_dir, size).await,
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

async fn cmd_devices() -> Result<()> {
    println!("Detecting storage devices...\n");

    let provider = platform_provider();
    match provider.list_devices() {
        Ok(devices) => {
            if devices.is_empty() {
                println!("No devices detected.");
                println!("Note: Raw device access usually requires elevated privileges.");
            } else {
                println!(
                    "{:<25} {:<30} {:>12} {:>10} {:<12} FS",
                    "Path", "Name", "Capacity", "Sector", "Type"
                );
                println!("{}", "─".repeat(100));
                for d in &devices {
                    println!(
                        "{:<25} {:<30} {:>12} {:>10} {:<12} {}",
                        d.path,
                        truncate(&d.name, 30),
                        format_bytes(d.size_bytes),
                        d.sector_size,
                        d.device_type.to_string(),
                        d.filesystem.as_deref().unwrap_or("—"),
                    );
                }
                println!("\n{} device(s) found.", devices.len());
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
    Ok(())
}

async fn cmd_sessions(action: SessionAction) -> Result<()> {
    match action {
        SessionAction::List { data_dir } => {
            let orchestrator = RecoveryOrchestrator::new(data_dir)?;
            let sessions = orchestrator.list_sessions()?;
            if sessions.is_empty() {
                println!("No sessions found.");
            } else {
                println!(
                    "{:<38} {:<12} {:<12} {:>10} Source",
                    "Session ID", "Status", "Mode", "Files"
                );
                println!("{}", "─".repeat(100));
                for s in &sessions {
                    println!(
                        "{:<38} {:<12} {:<12} {:>10} {}",
                        s.id,
                        s.status,
                        s.configuration.mode,
                        s.files_found,
                        truncate(&s.source_identity.path, 40),
                    );
                }
                println!("\n{} session(s).", sessions.len());
            }
        }

        SessionAction::Show {
            session_id,
            data_dir,
        } => {
            let orchestrator = RecoveryOrchestrator::new(data_dir)?;
            let id = session_id
                .parse::<SessionId>()
                .context("Invalid session ID")?;
            let s = orchestrator.load_session(&id)?;

            println!("Session: {}", s.id);
            println!("  Status:         {}", s.status);
            println!("  Mode:           {}", s.configuration.mode);
            println!("  Source:         {}", s.source_identity.path);
            println!("  Source size:    {}", format_bytes(s.source_size_bytes));
            println!("  Progress:       {:.1}%", s.progress_percent());
            println!("  Files found:    {}", s.files_found);
            println!("  Bad sectors:    {}", s.bad_sectors);
            println!(
                "  Created:        {}",
                s.created_at.format("%Y-%m-%d %H:%M:%S UTC")
            );
            if let Some(started) = s.started_at {
                println!(
                    "  Started:        {}",
                    started.format("%Y-%m-%d %H:%M:%S UTC")
                );
            }
            if let Some(error) = &s.last_error {
                println!("  Last error:     {}", error);
            }
        }

        SessionAction::Delete {
            session_id,
            data_dir,
        } => {
            let orchestrator = RecoveryOrchestrator::new(data_dir)?;
            let id = session_id
                .parse::<SessionId>()
                .context("Invalid session ID")?;
            orchestrator.delete_session(&id)?;
            println!("Session {} deleted.", session_id);
        }
    }
    Ok(())
}

async fn cmd_scan(
    source: String,
    mode: String,
    data_dir: PathBuf,
    size_hint: Option<u64>,
) -> Result<()> {
    println!("RecoverX — Scan");
    println!("  Source:    {}", source);
    println!("  Mode:      {}", mode);
    println!("  Data dir:  {}", data_dir.display());
    println!();

    // Determine source size
    let size_bytes = if let Some(s) = size_hint {
        s
    } else {
        std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0)
    };

    if size_bytes == 0 {
        eprintln!("Warning: could not determine source size. Use --size to specify.");
    }

    let config = match mode.to_lowercase().as_str() {
        "deep" => ScanConfiguration::deep(),
        "forensic" => ScanConfiguration::forensic(),
        "carving" | "file_carving" => ScanConfiguration::deep(), // placeholder
        _ => ScanConfiguration::quick(),
    };

    let identity = SourceIdentity {
        label: source.clone(),
        path: source.clone(),
        size_bytes,
        sector_size: 512,
        model: None,
        serial: None,
        filesystem_label: None,
        filesystem_type: None,
        partial_hash: None,
    };

    let orchestrator = RecoveryOrchestrator::new(data_dir)?;
    let session = orchestrator.create_session(identity.clone(), config)?;

    println!("Session created: {}", session.id);
    println!("Starting scan pipeline (Phase 1 — engine stubs)...\n");

    orchestrator.start_scan(&session.id, &identity).await?;

    let final_session = orchestrator.load_session(&session.id)?;
    println!("\nScan complete.");
    println!("  Status:      {}", final_session.status);
    println!("  Files found: {}", final_session.files_found);
    println!("  Progress:    {:.1}%", final_session.progress_percent());

    if let Some(err) = &final_session.last_error {
        eprintln!("  Error: {}", err);
    }

    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1_000_000_000;
    const MB: u64 = 1_000_000;
    const KB: u64 = 1_000;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}
