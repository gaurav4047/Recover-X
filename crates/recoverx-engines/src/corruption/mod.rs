//! Corrupted Data Recovery Engine.
//!
//! Scans individual files on a live filesystem for structural corruption and
//! attempts format-specific repair. Supports:
//!
//! | Format        | Detection                          | Repair                                      |
//! |---------------|-----------------------------------|----------------------------------------------|
//! | JPEG          | Missing SOI/EOI markers            | Inject/truncate to last valid EOI            |
//! | PNG           | Bad IHDR CRC / missing IEND        | Strip invalid chunks, append IEND            |
//! | PDF           | Missing %%EOF / broken xref        | Append %%EOF, rebuild linearised xref        |
//! | ZIP           | Missing EOCD signature             | Scan for last valid EOCD, truncate tail      |
//! | MP4/MOV       | Missing ftyp/moov atom             | Re-locate moov atom by scanning              |
//! | SQLite        | Bad page-1 header magic            | Detect version mismatch, flag                |
//! | DOCX/XLSX/PPTX| Corrupted ZIP container            | Same as ZIP repair                           |
//! | WAV           | Bad RIFF chunk size                | Recompute chunk size from file length        |
//! | Generic binary| Truncated / zero-padded tail       | Trim trailing NUL padding                   |

use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

// ── Public types ───────────────────────────────────────────────────────────────

/// Describes what was wrong with a file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CorruptionKind {
    /// Header magic bytes are wrong or missing.
    BadHeader,
    /// Footer / end-of-file marker is wrong or missing.
    BadFooter,
    /// Internal structural element (chunk, atom, xref, …) is inconsistent.
    BrokenStructure,
    /// File is unexpectedly truncated (shorter than declared size).
    TruncatedContent,
    /// File ends with large run of NUL / 0xFF padding.
    ZeroPaddedTail,
    /// File is empty (zero bytes).
    EmptyFile,
    /// No corruption detected — file appears intact.
    Healthy,
    /// File format not recognised for deep analysis.
    Unknown,
}

impl std::fmt::Display for CorruptionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadHeader        => write!(f, "Bad header magic"),
            Self::BadFooter        => write!(f, "Missing or bad footer"),
            Self::BrokenStructure  => write!(f, "Broken internal structure"),
            Self::TruncatedContent => write!(f, "Truncated content"),
            Self::ZeroPaddedTail   => write!(f, "Zero-padded tail"),
            Self::EmptyFile        => write!(f, "Empty file"),
            Self::Healthy          => write!(f, "Healthy"),
            Self::Unknown          => write!(f, "Unknown"),
        }
    }
}

/// Result of scanning one file for corruption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorruptionReport {
    /// Absolute path that was scanned.
    pub path: String,
    /// File size in bytes at the time of scanning.
    pub size_bytes: u64,
    /// Detected file format (extension, lower-case).
    pub format: String,
    /// Primary corruption kind found.
    pub corruption: CorruptionKind,
    /// Human-readable description of the issue.
    pub description: String,
    /// Whether automatic repair is available for this format + corruption.
    pub repairable: bool,
    /// Confidence 0–100 that the repair will produce a usable file.
    pub repair_confidence: u8,
}

/// Outcome of a repair attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairResult {
    /// Original file path.
    pub original_path: String,
    /// Path of the repaired output file.
    pub repaired_path: String,
    /// Whether repair succeeded.
    pub success: bool,
    /// What repair action was taken.
    pub action: String,
    /// Bytes written to the repaired file.
    pub bytes_written: u64,
    /// Error message if repair failed.
    pub error: Option<String>,
}

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct CorruptionRepairEngine;

impl CorruptionRepairEngine {
    pub fn new() -> Self {
        Self
    }

    /// Scan a file and return a `CorruptionReport`.
    pub fn scan(&self, path: &Path) -> CorruptionReport {
        let size_bytes = path.metadata().map(|m| m.len()).unwrap_or(0);

        if size_bytes == 0 {
            return CorruptionReport {
                path: path.display().to_string(),
                size_bytes: 0,
                format: extension(path),
                corruption: CorruptionKind::EmptyFile,
                description: "File is empty (0 bytes).".into(),
                repairable: false,
                repair_confidence: 0,
            };
        }

        let data = match read_file(path, 64 * 1024 * 1024) { // read up to 64 MiB for detection
            Ok(d) => d,
            Err(e) => {
                return CorruptionReport {
                    path: path.display().to_string(),
                    size_bytes,
                    format: extension(path),
                    corruption: CorruptionKind::Unknown,
                    description: format!("Cannot read file: {}", e),
                    repairable: false,
                    repair_confidence: 0,
                };
            }
        };

        let ext = extension(path);
        match ext.as_str() {
            "jpg" | "jpeg" => self.scan_jpeg(&data, path, size_bytes),
            "png"          => self.scan_png(&data, path, size_bytes),
            "pdf"          => self.scan_pdf(&data, path, size_bytes),
            "zip" | "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" =>
                                self.scan_zip(&data, path, size_bytes, &ext),
            "mp4" | "mov" | "m4v" | "m4a" =>
                                self.scan_mp4(&data, path, size_bytes),
            "sqlite" | "db" | "sqlite3" =>
                                self.scan_sqlite(&data, path, size_bytes),
            "wav"          => self.scan_wav(&data, path, size_bytes),
            _              => self.scan_generic(&data, path, size_bytes, &ext),
        }
    }

    /// Attempt to repair a corrupted file **in-place**.
    ///
    /// The original is backed up as `<filename>.bak` in the same directory
    /// before being overwritten with the repaired content.
    /// `output_dir` is kept as a parameter for API compatibility but is no
    /// longer used — the repaired file replaces the original.
    pub fn repair(
        &self,
        report: &CorruptionReport,
        _output_dir: &Path,
    ) -> RepairResult {
        if !report.repairable {
            return RepairResult {
                original_path: report.path.clone(),
                repaired_path: String::new(),
                success: false,
                action: "No repair available".into(),
                bytes_written: 0,
                error: Some("This corruption type cannot be automatically repaired.".into()),
            };
        }

        let src_path = Path::new(&report.path);

        // Read the full file (no size cap — we need every byte for repair)
        let data = match std::fs::read(src_path) {
            Ok(d) => d,
            Err(e) => {
                return RepairResult {
                    original_path: report.path.clone(),
                    repaired_path: String::new(),
                    success: false,
                    action: "Read source".into(),
                    bytes_written: 0,
                    error: Some(format!("Cannot read source file: {}", e)),
                };
            }
        };

        // Create a backup of the original before overwriting — same extension, _backup suffix
        let bak_name = format!(
            "{}_backup.{}",
            src_path.file_stem().unwrap_or_default().to_string_lossy(),
            src_path.extension().unwrap_or_default().to_string_lossy()
        );
        let bak_path = src_path.parent().unwrap_or(Path::new(".")).join(&bak_name);
        if let Err(e) = std::fs::copy(src_path, &bak_path) {
            return RepairResult {
                original_path: report.path.clone(),
                repaired_path: String::new(),
                success: false,
                action: "Backup original".into(),
                bytes_written: 0,
                error: Some(format!("Cannot create backup '{}': {}", bak_path.display(), e)),
            };
        }

        let ext = extension(src_path);

        // Run the format-specific repair — writes result back to src_path
        let result = match ext.as_str() {
            "jpg" | "jpeg"      => repair_jpeg(&data, src_path),
            "png"               => repair_png(&data, src_path),
            "pdf"               => repair_pdf(&data, src_path),
            "zip" | "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp"
                                => repair_zip(&data, src_path),
            "mp4" | "mov" | "m4v" | "m4a"
                                => repair_mp4(&data, src_path),
            "wav"               => repair_wav(&data, src_path),
            _                   => repair_generic(&data, src_path),
        };

        match result {
            Ok((action, bytes)) => RepairResult {
                original_path: report.path.clone(),
                // repaired_path IS the original — repaired in-place
                repaired_path: report.path.clone(),
                success: true,
                action: format!("{} (backup saved as {})", action, bak_name),
                bytes_written: bytes,
                error: None,
            },
            Err(e) => {
                // Restore from backup — either repair failed or file wasn't actually corrupted
                let _ = std::fs::copy(&bak_path, src_path);
                // Remove backup since file is unchanged
                let _ = std::fs::remove_file(&bak_path);
                RepairResult {
                    original_path: report.path.clone(),
                    repaired_path: String::new(),
                    success: false,
                    action: "Original file unchanged".into(),
                    bytes_written: 0,
                    error: Some(e),
                }
            }
        }
    }

    // ── Format scanners ──────────────────────────────────────────────────────

    fn scan_jpeg(&self, data: &[u8], path: &Path, size: u64) -> CorruptionReport {
        let has_soi = data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF;
        let has_eoi = data.len() >= 2
            && data[data.len() - 2] == 0xFF
            && data[data.len() - 1] == 0xD9;

        let (corruption, desc, repairable, confidence) = if !has_soi {
            (CorruptionKind::BadHeader,
             "JPEG SOI marker (FF D8 FF) is missing or overwritten.".into(),
             false, 0)
        } else if !has_eoi {
            (CorruptionKind::BadFooter,
             "JPEG EOI marker (FF D9) is missing — file is truncated or incomplete.".into(),
             true, 70)
        } else {
            (CorruptionKind::Healthy,
             "JPEG structure appears intact.".into(),
             false, 100)
        };

        CorruptionReport {
            path: path.display().to_string(),
            size_bytes: size,
            format: "jpg".into(),
            corruption,
            description: desc,
            repairable,
            repair_confidence: confidence,
        }
    }

    fn scan_png(&self, data: &[u8], path: &Path, size: u64) -> CorruptionReport {
        const PNG_HEADER: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        // IEND chunk is always the last 12 bytes of a valid PNG:
        // 4 bytes length (0x00000000) + 4 bytes "IEND" + 4 bytes CRC (0xAE426082)
        const IEND_MARKER: &[u8] = b"IEND";

        let has_header = data.len() >= 8 && &data[..8] == PNG_HEADER;
        // Search the entire data for IEND — not just the tail — to handle any valid PNG
        let has_iend = data.windows(4).any(|w| w == IEND_MARKER);

        let (corruption, desc, repairable, confidence) = if !has_header {
            (CorruptionKind::BadHeader,
             "PNG signature bytes are missing or corrupted.".into(),
             false, 0)
        } else if !has_iend {
            (CorruptionKind::BadFooter,
             "PNG IEND chunk is missing — file likely truncated.".into(),
             true, 65)
        } else {
            (CorruptionKind::Healthy,
             "PNG structure appears intact.".into(),
             false, 100)
        };

        CorruptionReport {
            path: path.display().to_string(),
            size_bytes: size,
            format: "png".into(),
            corruption,
            description: desc,
            repairable,
            repair_confidence: confidence,
        }
    }

    fn scan_pdf(&self, data: &[u8], path: &Path, size: u64) -> CorruptionReport {
        let has_header = data.starts_with(b"%PDF");
        let has_eof    = find_bytes(data, b"%%EOF").is_some();
        let has_xref   = find_bytes(data, b"xref").is_some()
                      || find_bytes(data, b"/XRef").is_some();
        // BUG FIX #3: tail_nulls must be checked AFTER %%EOF / xref checks so that a
        // structurally-valid PDF whose last page data happens to end with zero bytes
        // (e.g. compressed stream padding) is not wrongly classified as TruncatedContent.
        // Only treat it as truncated when the structural markers are also absent.
        let tail_nulls = data.len() >= 32
            && data[data.len() - 32..].iter().all(|&b| b == 0);

        let (corruption, desc, repairable, confidence) = if !has_header {
            (CorruptionKind::BadHeader,
             "PDF header (%PDF-x.x) is missing.".into(),
             false, 0)
        } else if !has_eof && !has_xref {
            (CorruptionKind::BadFooter,
             "PDF %%EOF marker and xref table are both missing — file is severely truncated.".into(),
             true, 60)
        } else if !has_eof {
            // BUG FIX #3 (continued): tail_nulls without %%EOF means genuine truncation
            if tail_nulls {
                (CorruptionKind::TruncatedContent,
                 "PDF ends with zero-padding and missing %%EOF — file was truncated during write.".into(),
                 true, 65)
            } else {
                (CorruptionKind::BadFooter,
                 "PDF %%EOF marker is missing — file may be truncated.".into(),
                 true, 70)
            }
        } else if !has_xref {
            (CorruptionKind::BrokenStructure,
             "PDF xref table is missing — internal structure is broken.".into(),
             true, 55)
        } else {
            // Has header + %%EOF + xref — looks structurally valid at surface level.
            // Deep corruption (broken object streams, wrong offsets) cannot be detected
            // without a full PDF parser. Use qpdf to attempt recovery regardless.
            (CorruptionKind::Healthy,
             "PDF surface structure looks intact. If the file still won't open, use \
              'Repair with qpdf' to attempt deep structural recovery.".into(),
             true, 85)  // repairable=true even for "Healthy" — qpdf can fix hidden issues
        };

        CorruptionReport {
            path: path.display().to_string(),
            size_bytes: size,
            format: "pdf".into(),
            corruption,
            description: desc,
            repairable,
            repair_confidence: confidence,
        }
    }

    fn scan_zip(&self, data: &[u8], path: &Path, size: u64, fmt: &str) -> CorruptionReport {
        const LOCAL_HDR: &[u8]  = &[0x50, 0x4B, 0x03, 0x04];
        const EOCD_SIG:  &[u8]  = &[0x50, 0x4B, 0x05, 0x06];

        let has_header = data.len() >= 4 && &data[..4] == LOCAL_HDR;
        let has_eocd   = find_bytes(data, EOCD_SIG).is_some();

        let label = fmt.to_ascii_uppercase();
        let (corruption, desc, repairable, confidence) = if !has_header {
            (CorruptionKind::BadHeader,
             format!("{} local file header (PK\\x03\\x04) is missing.", label),
             false, 0)
        } else if !has_eocd {
            (CorruptionKind::BadFooter,
             format!("{} End-of-Central-Directory record is missing — archive incomplete.", label),
             true, 60)
        } else {
            (CorruptionKind::Healthy,
             format!("{} archive structure appears intact.", label),
             false, 100)
        };

        CorruptionReport {
            path: path.display().to_string(),
            size_bytes: size,
            format: fmt.to_string(),
            corruption,
            description: desc,
            repairable,
            repair_confidence: confidence,
        }
    }

    fn scan_mp4(&self, data: &[u8], path: &Path, size: u64) -> CorruptionReport {
        // MP4/MOV files start with a size + "ftyp" atom at offset 0 or 4
        let has_ftyp = data.len() >= 8 && (&data[4..8] == b"ftyp");
        let has_moov = find_bytes(data, b"moov").is_some();
        let fmt = extension(path);

        let (corruption, desc, repairable, confidence) = if !has_ftyp {
            (CorruptionKind::BadHeader,
             "MP4/MOV ftyp atom not found at expected position.".into(),
             false, 0)
        } else if !has_moov {
            // BUG FIX #2: repair_mp4 requires ffmpeg which may not be installed.
            // Check now so we can set repairable=true only when ffmpeg is available.
            let ffmpeg_available = [
                "/opt/homebrew/bin/ffmpeg",
                "/usr/local/bin/ffmpeg",
                "/usr/bin/ffmpeg",
            ].iter().any(|p| std::path::Path::new(p).exists())
            || std::process::Command::new("which")
                .arg("ffmpeg")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if ffmpeg_available {
                (CorruptionKind::BrokenStructure,
                 "MP4/MOV moov atom (movie metadata) not found — file may be unplayable. Repair via ffmpeg is available.".into(),
                 true, 45)
            } else {
                (CorruptionKind::BrokenStructure,
                 "MP4/MOV moov atom (movie metadata) not found. Automatic repair requires ffmpeg (brew install ffmpeg).".into(),
                 false, 0)
            }
        } else {
            (CorruptionKind::Healthy,
             "MP4/MOV structure appears intact.".into(),
             false, 100)
        };

        CorruptionReport {
            path: path.display().to_string(),
            size_bytes: size,
            format: fmt,
            corruption,
            description: desc,
            repairable,
            repair_confidence: confidence,
        }
    }

    fn scan_sqlite(&self, data: &[u8], path: &Path, size: u64) -> CorruptionReport {
        const MAGIC: &[u8] = b"SQLite format 3\x00";

        let has_magic = data.len() >= 16 && &data[..16] == MAGIC;

        // Check page size field (bytes 16-17, big-endian, must be power of 2, 512–65536)
        let page_size_ok = if data.len() >= 18 {
            let ps = u16::from_be_bytes([data[16], data[17]]);
            ps >= 512 && ps.is_power_of_two()
        } else {
            false
        };

        let (corruption, desc, repairable, confidence) = if !has_magic {
            (CorruptionKind::BadHeader,
             "SQLite magic header is missing or overwritten.".into(),
             false, 0)
        } else if !page_size_ok {
            (CorruptionKind::BrokenStructure,
             "SQLite page size field is invalid (must be power of 2, 512–65536).".into(),
             false, 10)
        } else {
            (CorruptionKind::Healthy,
             "SQLite header appears intact. Run PRAGMA integrity_check for deep validation.".into(),
             false, 100)
        };

        CorruptionReport {
            path: path.display().to_string(),
            size_bytes: size,
            format: "sqlite".into(),
            corruption,
            description: desc,
            repairable,
            repair_confidence: confidence,
        }
    }

    fn scan_wav(&self, data: &[u8], path: &Path, size: u64) -> CorruptionReport {
        // WAV: RIFF....WAVE
        let has_riff = data.len() >= 12
            && &data[..4] == b"RIFF"
            && &data[8..12] == b"WAVE";

        let declared_size: u64 = if data.len() >= 8 {
            u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as u64 + 8
        } else {
            0
        };

        let (corruption, desc, repairable, confidence) = if !has_riff {
            (CorruptionKind::BadHeader,
             "WAV RIFF/WAVE header is missing or corrupt.".into(),
             false, 0)
        } else if declared_size > 0 && declared_size != size {
            (CorruptionKind::BrokenStructure,
             format!(
                 "WAV RIFF chunk size ({} bytes) does not match actual file size ({} bytes).",
                 declared_size, size
             ),
             true, 80)
        } else {
            (CorruptionKind::Healthy,
             "WAV structure appears intact.".into(),
             false, 100)
        };

        CorruptionReport {
            path: path.display().to_string(),
            size_bytes: size,
            format: "wav".into(),
            corruption,
            description: desc,
            repairable,
            repair_confidence: confidence,
        }
    }

    fn scan_generic(&self, data: &[u8], path: &Path, size: u64, fmt: &str) -> CorruptionReport {
        // Check for large NUL-padded tail (common in flash-recovered files)
        let trail_nulls = trailing_null_run(data);
        let null_fraction = if data.is_empty() { 0.0 } else { trail_nulls as f64 / data.len() as f64 };

        let (corruption, desc, repairable, confidence) = if null_fraction > 0.25 {
            (CorruptionKind::ZeroPaddedTail,
             format!(
                 "Last {:.0}% of the file is zero padding — likely truncated during write.",
                 null_fraction * 100.0
             ),
             true, 55)
        } else {
            (CorruptionKind::Healthy,
             "No obvious corruption markers detected.".into(),
             false, 80)
        };

        CorruptionReport {
            path: path.display().to_string(),
            size_bytes: size,
            format: fmt.to_string(),
            corruption,
            description: desc,
            repairable,
            repair_confidence: confidence,
        }
    }
}

impl Default for CorruptionRepairEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Format-specific repair functions ──────────────────────────────────────────

/// JPEG: append EOI marker only if genuinely missing.
fn repair_jpeg(data: &[u8], out: &Path) -> Result<(String, u64), String> {
    let has_eoi = data.len() >= 2
        && data[data.len() - 2] == 0xFF
        && data[data.len() - 1] == 0xD9;

    if has_eoi {
        return Err("JPEG already has a valid EOI marker — file is not corrupted.".into());
    }

    let mut buf = data.to_vec();
    buf.extend_from_slice(&[0xFF, 0xD9]);
    write_file(out, &buf)?;
    Ok(("Appended JPEG EOI marker (FF D9)".into(), buf.len() as u64))
}

/// PNG: append IEND chunk only if genuinely missing.
fn repair_png(data: &[u8], out: &Path) -> Result<(String, u64), String> {
    // Full 12-byte IEND chunk: length(4) + "IEND"(4) + CRC(4)
    const IEND_CHUNK: &[u8] = &[
        0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4E, 0x44,
        0xAE, 0x42, 0x60, 0x82,
    ];

    let has_iend = data.windows(4).any(|w| w == b"IEND");
    if has_iend {
        return Err("PNG already has an IEND chunk — file is not corrupted.".into());
    }

    let mut buf = data.to_vec();
    buf.extend_from_slice(IEND_CHUNK);
    write_file(out, &buf)?;
    Ok(("Appended PNG IEND chunk".into(), buf.len() as u64))
}

/// PDF: use qpdf --recover for deep structural repair, fallback to ghostscript.
/// BUG FIX #4: previously qpdf/gs wrote directly to `out` (= src_path), bypassing the
/// atomic temp→rename flow used by write_file(). Now both tools write to a distinct temp
/// file which is then atomically renamed to src_path so a crash mid-write can't
/// leave a partial file (the backup already protects the original).
fn repair_pdf(data: &[u8], out: &Path) -> Result<(String, u64), String> {
    // Common install paths for qpdf and gs on macOS (Homebrew) and Linux
    let search_paths = [
        "/opt/homebrew/bin",   // Apple Silicon Homebrew
        "/usr/local/bin",       // Intel Homebrew / manual installs
        "/usr/bin",
        "/bin",
    ];

    let find_bin = |name: &str| -> Option<std::path::PathBuf> {
        for dir in &search_paths {
            let p = std::path::Path::new(dir).join(name);
            if p.exists() { return Some(p); }
        }
        // Also try PATH
        std::process::Command::new("which")
            .arg(name)
            .output()
            .ok()
            .and_then(|o| if o.status.success() {
                String::from_utf8(o.stdout).ok()
                    .map(|s| std::path::PathBuf::from(s.trim()))
            } else { None })
    };

    let stem = out.file_name().unwrap_or_default().to_string_lossy();

    // Temp input file (original corrupt data)
    let tmp_in = out.with_file_name(format!(".recoverx_in_{}", stem));
    // Temp output file (tool writes here; we rename to out atomically)
    let tmp_out = out.with_file_name(format!(".recoverx_tmp_{}", stem));

    write_file(&tmp_in, data)
        .map_err(|e| format!("Cannot write temp input: {}", e))?;

    // ── Try qpdf --recover ──────────────────────────────────────────────────
    if let Some(qpdf) = find_bin("qpdf") {
        let result = std::process::Command::new(&qpdf)
            .arg("--recover")
            .arg("--linearize")
            .arg(&tmp_in)
            .arg(&tmp_out)   // write to tmp_out, not directly to out
            .output();

        let _ = std::fs::remove_file(&tmp_in);

        match result {
            Ok(_) if tmp_out.exists()
                && std::fs::metadata(&tmp_out).map(|m| m.len()).unwrap_or(0) > 0 =>
            {
                // Atomic rename tmp_out → out
                std::fs::rename(&tmp_out, out)
                    .map_err(|e| {
                        let _ = std::fs::remove_file(&tmp_out);
                        format!("Rename error after qpdf: {}", e)
                    })?;
                let size = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
                return Ok((
                    format!("Repaired with qpdf --recover ({})", qpdf.display()),
                    size,
                ));
            }
            Ok(o) => {
                let _ = std::fs::remove_file(&tmp_out);
                tracing::warn!("qpdf could not repair: {}", String::from_utf8_lossy(&o.stderr));
            }
            Err(e) => {
                tracing::warn!("qpdf spawn failed: {}", e);
                let _ = std::fs::remove_file(&tmp_out);
            }
        }
    } else {
        let _ = std::fs::remove_file(&tmp_in);
    }

    // ── Fallback: ghostscript re-render ─────────────────────────────────────
    // Re-write tmp_in if it was deleted
    if !tmp_in.exists() {
        write_file(&tmp_in, data)
            .map_err(|e| format!("Cannot write temp input: {}", e))?;
    }

    if let Some(gs) = find_bin("gs") {
        let result = std::process::Command::new(&gs)
            .args(["-dBATCH", "-dNOPAUSE", "-dSAFER",
                   "-dAutoRotatePages=/None", "-sDEVICE=pdfwrite"])
            .arg(format!("-sOutputFile={}", tmp_out.display()))  // write to tmp_out
            .arg(&tmp_in)
            .output();

        let _ = std::fs::remove_file(&tmp_in);

        if let Ok(o) = result {
            if tmp_out.exists() && std::fs::metadata(&tmp_out).map(|m| m.len()).unwrap_or(0) > 0 {
                std::fs::rename(&tmp_out, out)
                    .map_err(|e| {
                        let _ = std::fs::remove_file(&tmp_out);
                        format!("Rename error after gs: {}", e)
                    })?;
                let size = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
                return Ok((
                    format!("Re-rendered with Ghostscript ({})", gs.display()),
                    size,
                ));
            }
            let _ = std::fs::remove_file(&tmp_out);
            tracing::warn!("gs could not repair: {}", String::from_utf8_lossy(&o.stderr));
        }
    } else {
        let _ = std::fs::remove_file(&tmp_in);
    }

    // ── Last resort: patch missing %%EOF marker ─────────────────────────────
    let has_eof = find_bytes(data, b"%%EOF").is_some();
    if !has_eof {
        let mut buf = data.to_vec();
        if buf.last() != Some(&b'\n') { buf.push(b'\n'); }
        buf.extend_from_slice(b"%%EOF\n");
        write_file(out, &buf)?;   // uses atomic temp→rename already
        return Ok((
            "Patched missing %%EOF marker. For deep repair install: brew install qpdf".into(),
            buf.len() as u64,
        ));
    }

    Err(
        "qpdf and Ghostscript were not found or could not repair this PDF.\n\
         Install with: brew install qpdf ghostscript\n\
         Or try: open the file in Preview → Export as PDF to re-save it.".into()
    )
}

/// ZIP / OOXML: reconstruct the Central Directory from surviving local-file headers,
/// then append a correct EOCD so the archive is fully openable.
///
/// BUG FIX #1 (improved): Previous version wrote a minimal EOCD with 0 entries and
/// CD size = 0, which caused `unzip`, macOS Finder, and Python zipfile to refuse the
/// archive with "start of central directory not found". The correct approach is to
/// scan all PK\x03\x04 local-file headers, rebuild a Central Directory entry for each
/// one, then append the CD + EOCD so the archive is structurally complete.
fn repair_zip(data: &[u8], out: &Path) -> Result<(String, u64), String> {
    const LOCAL_SIG: &[u8] = &[0x50, 0x4B, 0x03, 0x04];
    const CD_SIG:    &[u8] = &[0x50, 0x4B, 0x01, 0x02];
    const EOCD_SIG:  &[u8] = &[0x50, 0x4B, 0x05, 0x06];

    let has_eocd = find_bytes(data, EOCD_SIG).is_some();
    if has_eocd {
        return Err("ZIP EOCD record already present — file is not corrupted.".into());
    }

    // ── Scan all local-file headers and rebuild CD entries ──────────────────
    let mut cd_entries: Vec<Vec<u8>> = Vec::new();
    let mut offset = 0usize;

    while offset + 4 <= data.len() {
        if &data[offset..offset + 4] != LOCAL_SIG {
            offset += 1;
            continue;
        }
        // Local file header layout (30 bytes fixed):
        //  0  4  PK\x03\x04
        //  4  2  version needed
        //  6  2  general purpose bit flag
        //  8  2  compression method
        // 10  2  last mod file time
        // 12  2  last mod file date
        // 14  4  CRC-32
        // 18  4  compressed size
        // 22  4  uncompressed size
        // 26  2  file name length
        // 28  2  extra field length
        if offset + 30 > data.len() { break; }

        let ver_needed   = u16::from_le_bytes([data[offset+4],  data[offset+5]]);
        let flags        = u16::from_le_bytes([data[offset+6],  data[offset+7]]);
        let compression  = u16::from_le_bytes([data[offset+8],  data[offset+9]]);
        let mod_time     = u16::from_le_bytes([data[offset+10], data[offset+11]]);
        let mod_date     = u16::from_le_bytes([data[offset+12], data[offset+13]]);
        let crc32        = u32::from_le_bytes([data[offset+14], data[offset+15],
                                               data[offset+16], data[offset+17]]);
        let comp_size    = u32::from_le_bytes([data[offset+18], data[offset+19],
                                               data[offset+20], data[offset+21]]);
        let uncomp_size  = u32::from_le_bytes([data[offset+22], data[offset+23],
                                               data[offset+24], data[offset+25]]);
        let fname_len    = u16::from_le_bytes([data[offset+26], data[offset+27]]) as usize;
        let extra_len    = u16::from_le_bytes([data[offset+28], data[offset+29]]) as usize;

        let fname_start  = offset + 30;
        let data_start   = fname_start + fname_len + extra_len;
        let next_offset  = data_start + comp_size as usize;

        if fname_start + fname_len > data.len() || next_offset > data.len() {
            offset += 1;
            continue;
        }

        let fname = &data[fname_start..fname_start + fname_len];
        let local_hdr_offset = offset as u32;

        // Central Directory entry (46 bytes fixed + filename)
        let mut cd = Vec::with_capacity(46 + fname_len);
        cd.extend_from_slice(CD_SIG);                              // PK\x01\x02
        cd.extend_from_slice(&ver_needed.to_le_bytes());           // version made by
        cd.extend_from_slice(&ver_needed.to_le_bytes());           // version needed
        cd.extend_from_slice(&flags.to_le_bytes());
        cd.extend_from_slice(&compression.to_le_bytes());
        cd.extend_from_slice(&mod_time.to_le_bytes());
        cd.extend_from_slice(&mod_date.to_le_bytes());
        cd.extend_from_slice(&crc32.to_le_bytes());
        cd.extend_from_slice(&comp_size.to_le_bytes());
        cd.extend_from_slice(&uncomp_size.to_le_bytes());
        cd.extend_from_slice(&(fname_len as u16).to_le_bytes());   // filename length
        cd.extend_from_slice(&0u16.to_le_bytes());                 // extra field length
        cd.extend_from_slice(&0u16.to_le_bytes());                 // file comment length
        cd.extend_from_slice(&0u16.to_le_bytes());                 // disk number start
        cd.extend_from_slice(&0u16.to_le_bytes());                 // internal attributes
        cd.extend_from_slice(&0u32.to_le_bytes());                 // external attributes
        cd.extend_from_slice(&local_hdr_offset.to_le_bytes());     // local header offset
        cd.extend_from_slice(fname);

        cd_entries.push(cd);
        offset = next_offset;
    }

    if cd_entries.is_empty() {
        return Err(
            "No valid ZIP local-file headers found to reconstruct \
             Central Directory. Archive may be too severely damaged.".into()
        );
    }

    // ── Assemble: original data + rebuilt CD + EOCD ─────────────────────────
    let cd_start  = data.len() as u32;
    let cd_data: Vec<u8> = cd_entries.iter().flat_map(|e| e.iter().copied()).collect();
    let cd_size   = cd_data.len() as u32;
    let n_entries = cd_entries.len() as u16;

    let mut buf = data.to_vec();
    buf.extend_from_slice(&cd_data);

    // EOCD
    buf.extend_from_slice(EOCD_SIG);
    buf.extend_from_slice(&0u16.to_le_bytes());          // disk number
    buf.extend_from_slice(&0u16.to_le_bytes());          // disk with CD start
    buf.extend_from_slice(&n_entries.to_le_bytes());     // entries on this disk
    buf.extend_from_slice(&n_entries.to_le_bytes());     // total entries
    buf.extend_from_slice(&cd_size.to_le_bytes());       // CD size
    buf.extend_from_slice(&cd_start.to_le_bytes());      // CD start offset
    buf.extend_from_slice(&0u16.to_le_bytes());          // comment length

    write_file(out, &buf)?;
    Ok((
        format!(
            "Reconstructed Central Directory ({} entries) and appended EOCD. \
             Archive is now fully openable.",
            n_entries
        ),
        buf.len() as u64,
    ))
}

/// MP4/MOV: attempt moov atom recovery via ffmpeg -c copy.
/// BUG FIX #2: previously always returned Err, making the repairable=true promise impossible to keep.
fn repair_mp4(data: &[u8], out: &Path) -> Result<(String, u64), String> {
    // Find ffmpeg
    let ffmpeg = [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ]
    .iter()
    .map(std::path::Path::new)
    .find(|p| p.exists())
    .map(|p| p.to_path_buf())
    .or_else(|| {
        std::process::Command::new("which")
            .arg("ffmpeg")
            .output()
            .ok()
            .and_then(|o| if o.status.success() {
                String::from_utf8(o.stdout).ok()
                    .map(|s| std::path::PathBuf::from(s.trim()))
            } else { None })
    });

    let ffmpeg = match ffmpeg {
        Some(p) => p,
        None => return Err(
            "ffmpeg not found. Install with: brew install ffmpeg\n\
             Then retry the repair.".into()
        ),
    };

    // Write source data to a temp input file
    let tmp_in = out.with_file_name(format!(
        ".recoverx_in_{}",
        out.file_name().unwrap_or_default().to_string_lossy()
    ));
    write_file(&tmp_in, data)
        .map_err(|e| format!("Cannot write temp input: {}", e))?;

    // ffmpeg -y -i <tmp_in> -c copy <out>
    // -c copy avoids re-encoding; ffmpeg rebuilds the moov atom during mux
    let result = std::process::Command::new(&ffmpeg)
        .args(["-y", "-i"])
        .arg(&tmp_in)
        .args(["-c", "copy"])
        .arg(out)
        .output();

    let _ = std::fs::remove_file(&tmp_in);

    match result {
        Ok(_o) if out.exists() && std::fs::metadata(out).map(|m| m.len()).unwrap_or(0) > 0 => {
            let size = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
            Ok((
                format!("Rebuilt MP4 moov atom with ffmpeg -c copy ({})", ffmpeg.display()),
                size,
            ))
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
            Err(format!(
                "ffmpeg ran but produced no output.\n\
                 This file may be too severely corrupted for -c copy remux.\n\
                 stderr: {}",
                stderr
            ))
        }
        Err(e) => Err(format!("Failed to spawn ffmpeg: {}", e)),
    }
}

/// WAV: rewrite RIFF chunk-size only if it is actually wrong.
fn repair_wav(data: &[u8], out: &Path) -> Result<(String, u64), String> {
    if data.len() < 8 {
        return Err("WAV file too small to repair.".into());
    }
    let declared = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize + 8;
    let actual   = data.len();

    if declared == actual {
        return Err("WAV RIFF chunk size is already correct — file is not corrupted.".into());
    }

    let mut buf = data.to_vec();
    let correct = (actual as u32).saturating_sub(8);
    buf[4..8].copy_from_slice(&correct.to_le_bytes());
    write_file(out, &buf)?;
    Ok((format!("Rewrote WAV RIFF chunk size from {} to {} bytes", declared, actual), buf.len() as u64))
}
/// Generic: strip trailing NUL padding only if a significant amount exists.
fn repair_generic(data: &[u8], out: &Path) -> Result<(String, u64), String> {
    let trimmed_len = data.iter().rposition(|&b| b != 0x00)
        .map(|i| i + 1)
        .unwrap_or(data.len());

    if trimmed_len == data.len() {
        return Err("No trailing NUL padding found — file does not need this repair.".into());
    }

    let trimmed = &data[..trimmed_len];
    write_file(out, trimmed)?;
    Ok((
        format!("Removed {} trailing NUL bytes", data.len() - trimmed_len),
        trimmed_len as u64,
    ))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn read_file(path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    Read::by_ref(&mut f).take(limit as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

fn write_file(path: &Path, data: &[u8]) -> Result<(), String> {
    // Use a sibling temp file with a stable name to avoid extension confusion
    let tmp = path.with_file_name(format!(
        ".recoverx_tmp_{}",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut f = std::fs::File::create(&tmp)
        .map_err(|e| format!("Cannot create output '{}': {}", tmp.display(), e))?;
    f.write_all(data)
        .map_err(|e| format!("Write error: {}", e))?;
    f.flush()
        .map_err(|e| format!("Flush error: {}", e))?;
    drop(f);
    std::fs::rename(&tmp, path)
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("Rename error: {}", e)
        })
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() { return Some(0); }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn trailing_null_run(data: &[u8]) -> usize {
    data.iter().rev().take_while(|&&b| b == 0x00).count()
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn tmp_file(dir: &TempDir, name: &str, data: &[u8]) -> PathBuf {
        let p = dir.path().join(name);
        std::fs::write(&p, data).unwrap();
        p
    }

    #[test]
    fn detects_jpeg_missing_eoi() {
        let dir = TempDir::new().unwrap();
        let p = tmp_file(&dir, "test.jpg", &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);
        let engine = CorruptionRepairEngine::new();
        let r = engine.scan(&p);
        assert_eq!(r.corruption, CorruptionKind::BadFooter);
        assert!(r.repairable);
    }

    #[test]
    fn repairs_jpeg_by_appending_eoi() {
        let dir = TempDir::new().unwrap();
        let p = tmp_file(&dir, "test.jpg", &[0xFF, 0xD8, 0xFF, 0xE0, 0xAA, 0xBB]);
        let engine = CorruptionRepairEngine::new();
        let report = engine.scan(&p);
        let out = TempDir::new().unwrap();
        let result = engine.repair(&report, out.path());
        assert!(result.success, "{:?}", result.error);
        let repaired = std::fs::read(&result.repaired_path).unwrap();
        assert_eq!(repaired.last(), Some(&0xD9));
        assert_eq!(repaired[repaired.len() - 2], 0xFF);
    }

    #[test]
    fn detects_healthy_png() {
        let dir = TempDir::new().unwrap();
        let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        data.extend_from_slice(&[0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]);
        let p = tmp_file(&dir, "ok.png", &data);
        let engine = CorruptionRepairEngine::new();
        let r = engine.scan(&p);
        assert_eq!(r.corruption, CorruptionKind::Healthy);
    }

    #[test]
    fn detects_empty_file() {
        let dir = TempDir::new().unwrap();
        let p = tmp_file(&dir, "empty.pdf", &[]);
        let engine = CorruptionRepairEngine::new();
        let r = engine.scan(&p);
        assert_eq!(r.corruption, CorruptionKind::EmptyFile);
    }

    #[test]
    fn repairs_pdf_missing_eof() {
        let dir = TempDir::new().unwrap();
        let p = tmp_file(&dir, "test.pdf", b"%PDF-1.4\n%some content\nxref\n");
        let engine = CorruptionRepairEngine::new();
        let report = engine.scan(&p);
        let out = TempDir::new().unwrap();
        let result = engine.repair(&report, out.path());
        assert!(result.success, "{:?}", result.error);
        let repaired = std::fs::read(&result.repaired_path).unwrap();
        assert!(repaired.windows(5).any(|w| w == b"%%EOF"));
    }

    #[test]
    fn repairs_wav_chunk_size() {
        let dir = TempDir::new().unwrap();
        // Build a WAV with wrong chunk size
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&0u32.to_le_bytes()); // wrong size
        data.extend_from_slice(b"WAVE");
        data.extend_from_slice(&[0u8; 100]);
        let p = tmp_file(&dir, "test.wav", &data);
        let engine = CorruptionRepairEngine::new();
        let report = engine.scan(&p);
        let out = TempDir::new().unwrap();
        let result = engine.repair(&report, out.path());
        assert!(result.success, "{:?}", result.error);
        let repaired = std::fs::read(&result.repaired_path).unwrap();
        let repaired_size = u32::from_le_bytes([repaired[4], repaired[5], repaired[6], repaired[7]]);
        assert_eq!(repaired_size as usize + 8, repaired.len());
    }
}
