//! File carving engine.
//!
//! Scans raw sectors looking for file headers (magic bytes). When a header is
//! found, the engine attempts to locate the matching footer or uses a known
//! fixed size to bound the file. Each carved file is assigned a confidence
//! score based on structure validity.

use recoverx_core::error::Result;
use recoverx_storage::source::StorageSource;
use uuid::Uuid;

use crate::models::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};

/// A file type signature entry.
#[derive(Debug, Clone)]
pub struct FileSignature {
    /// File extension
    pub extension: &'static str,
    /// MIME type
    pub mime_type: &'static str,
    /// Header bytes (magic)
    pub header: &'static [u8],
    /// Byte offset within the file where the header appears (usually 0)
    pub header_offset: usize,
    /// Footer bytes, if known
    pub footer: Option<&'static [u8]>,
    /// Maximum file size to carve (bytes). Prevents unbounded reads.
    pub max_size: u64,
    /// Fixed size if all files of this type have exact same size; otherwise 0.
    pub fixed_size: u64,
}

/// The carving engine.
pub struct FileCarver {
    signatures: Vec<FileSignature>,
}

impl FileCarver {
    /// Create a carver with the built-in signature database.
    pub fn new() -> Self {
        FileCarver {
            signatures: signature_database(),
        }
    }

    /// Carve files from `source`, scanning from byte 0 to `total_bytes`.
    pub fn carve(
        &self,
        source: &mut dyn StorageSource,
        session_id: &str,
        total_bytes: u64,
    ) -> Result<Vec<RecoveredFile>> {
        let mut results = Vec::new();
        let _sector_size = source.sector_size() as u64;
        let actual_size = source.size().min(total_bytes);

        if actual_size == 0 {
            return Ok(results);
        }

        // Read in 1 MiB chunks with 64-byte overlap to catch signatures at boundaries
        let chunk_size: u64 = 1024 * 1024;
        let overlap: u64 = 64;
        let max_carved = 10_000usize;

        let mut pos: u64 = 0;

        while pos < actual_size && results.len() < max_carved {
            let read_end = (pos + chunk_size + overlap).min(actual_size);
            let read_size = (read_end - pos) as usize;
            let mut chunk = Vec::new();

            match source.read(pos, read_size, &mut chunk) {
                Ok(n) if n > 0 => {}
                _ => {
                    pos += chunk_size;
                    continue;
                }
            }

            // Search for each signature in this chunk
            for sig in &self.signatures {
                if sig.header.is_empty() { continue; }

                let mut search_pos = sig.header_offset;
                while search_pos + sig.header.len() <= chunk.len() {
                    if &chunk[search_pos..search_pos + sig.header.len()] == sig.header {
                        let abs_offset = pos + search_pos as u64 - sig.header_offset as u64;

                        // Don't double-carve the same offset
                        if results.iter().any(|r: &RecoveredFile| r.source_offset == abs_offset) {
                            search_pos += sig.header.len();
                            continue;
                        }

                        // Determine file size
                        let (file_size, status, confidence) = self.estimate_size(
                            source,
                            abs_offset,
                            sig,
                            actual_size,
                        );

                        if file_size == 0 {
                            search_pos += sig.header.len();
                            continue;
                        }

                        let carved_index = results.len();
                        let name = format!("carved_{:07}.{}", carved_index, sig.extension);

                        results.push(RecoveredFile {
                            id: Uuid::new_v4(),
                            session_id: session_id.to_string(),
                            name,
                            original_path: None, // carving has no path info
                            size_bytes: file_size,
                            source_offset: abs_offset,
                            partition_index: None,
                            filesystem_type: None,
                            recovery_method: RecoveryMethod::FileCarving,
                            confidence,
                            status,
                            sha256: None,
                            created_at: None,
                            modified_at: None,
                            accessed_at: None,
                            extension: Some(sig.extension.to_string()),
                            mime_type: Some(sig.mime_type.to_string()),
                            is_deleted: true,
                            is_fragmented: false,
                            fragments: vec![FileFragment {
                                offset: abs_offset,
                                length: file_size,
                                order: 0,
                            }],
                            metadata: serde_json::json!({
                                "carved": true,
                                "signature": sig.extension,
                            }),
                        });

                        if results.len() >= max_carved { break; }
                    }
                    search_pos += 1;
                }

                if results.len() >= max_carved { break; }
            }

            pos += chunk_size;
        }

        Ok(results)
    }

    /// Estimate file size given a found header at `offset`.
    fn estimate_size(
        &self,
        source: &mut dyn StorageSource,
        offset: u64,
        sig: &FileSignature,
        source_size: u64,
    ) -> (u64, FileStatus, u8) {
        // Fixed-size file (e.g., BMP with size field)
        if sig.fixed_size > 0 {
            let actual = sig.fixed_size.min(source_size - offset);
            return (actual, FileStatus::Complete, 80);
        }

        // Try to parse size from file header for known formats
        if let Some(size) = self.parse_embedded_size(source, offset, sig) {
            let capped = size.min(sig.max_size).min(source_size - offset);
            return (capped, FileStatus::Complete, 85);
        }

        // Footer-based carving
        if let Some(footer) = sig.footer {
            if let Some(footer_pos) = self.find_footer(source, offset + sig.header.len() as u64, sig.max_size, footer, source_size) {
                let size = footer_pos - offset + footer.len() as u64;
                return (size, FileStatus::Complete, 75);
            }
            // Footer not found — partial
            let size = sig.max_size.min(source_size - offset);
            return (size, FileStatus::Partial, 40);
        }

        // No footer and no embedded size — use max_size cap
        let size = sig.max_size.min(source_size - offset);
        (size, FileStatus::Partial, 30)
    }

    /// Scan forward looking for footer bytes.
    fn find_footer(
        &self,
        source: &mut dyn StorageSource,
        start: u64,
        max_size: u64,
        footer: &[u8],
        source_size: u64,
    ) -> Option<u64> {
        let search_end = (start + max_size).min(source_size);
        let chunk_size = 65536usize;
        let overlap = footer.len();

        let mut pos = start;
        while pos < search_end {
            let read_size = (chunk_size + overlap).min((search_end - pos) as usize);
            let mut buf = Vec::new();
            if source.read(pos, read_size, &mut buf).is_err() {
                break;
            }

            for i in 0..buf.len().saturating_sub(footer.len() - 1) {
                if &buf[i..i + footer.len()] == footer {
                    return Some(pos + i as u64);
                }
            }

            pos += chunk_size as u64;
        }
        None
    }

    /// Try to read a size value embedded in the file header for known formats.
    fn parse_embedded_size(
        &self,
        source: &mut dyn StorageSource,
        offset: u64,
        sig: &FileSignature,
    ) -> Option<u64> {
        // Read first 64 bytes for header parsing
        let mut hdr = Vec::new();
        if source.read(offset, 64, &mut hdr).ok()? < 8 {
            return None;
        }

        match sig.extension {
            "bmp" => {
                // BMP: file size at bytes 2-5 (little-endian u32)
                if hdr.len() >= 6 {
                    let sz = u32::from_le_bytes([hdr[2], hdr[3], hdr[4], hdr[5]]) as u64;
                    if sz > 0 && sz < sig.max_size { return Some(sz); }
                }
            }
            "wav" => {
                // WAV: RIFF chunk size at bytes 4-7 + 8 bytes for RIFF header
                if hdr.len() >= 8 {
                    let chunk_size = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as u64;
                    let total = chunk_size + 8;
                    if total < sig.max_size { return Some(total); }
                }
            }
            _ => {}
        }

        None
    }
}

impl Default for FileCarver {
    fn default() -> Self {
        Self::new()
    }
}

/// The built-in file signature database.
pub fn signature_database() -> Vec<FileSignature> {
    vec![
        // ── Images ────────────────────────────────────────────────────────
        FileSignature {
            extension: "jpg",
            mime_type: "image/jpeg",
            header: &[0xFF, 0xD8, 0xFF],
            header_offset: 0,
            footer: Some(&[0xFF, 0xD9]),
            max_size: 50 * 1024 * 1024, // 50 MB
            fixed_size: 0,
        },
        FileSignature {
            extension: "png",
            mime_type: "image/png",
            header: &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            header_offset: 0,
            footer: Some(&[0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]),
            max_size: 100 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "gif",
            mime_type: "image/gif",
            header: b"GIF89a",
            header_offset: 0,
            footer: Some(&[0x00, 0x3B]),
            max_size: 20 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "bmp",
            mime_type: "image/bmp",
            header: &[0x42, 0x4D],
            header_offset: 0,
            footer: None,
            max_size: 100 * 1024 * 1024,
            fixed_size: 0, // size read from header
        },
        FileSignature {
            extension: "tiff",
            mime_type: "image/tiff",
            header: &[0x49, 0x49, 0x2A, 0x00], // little-endian
            header_offset: 0,
            footer: None,
            max_size: 200 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "webp",
            mime_type: "image/webp",
            header: b"RIFF",
            header_offset: 0,
            footer: None,
            max_size: 50 * 1024 * 1024,
            fixed_size: 0,
        },
        // ── Documents ─────────────────────────────────────────────────────
        FileSignature {
            extension: "pdf",
            mime_type: "application/pdf",
            header: b"%PDF",
            header_offset: 0,
            footer: Some(b"%%EOF"),
            max_size: 500 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "docx",
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            header: &[0x50, 0x4B, 0x03, 0x04], // ZIP (OOXML)
            header_offset: 0,
            footer: Some(&[0x50, 0x4B, 0x05, 0x06]),
            max_size: 100 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "doc",
            mime_type: "application/msword",
            header: &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1], // OLE2
            header_offset: 0,
            footer: None,
            max_size: 100 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "xlsx",
            mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            header: &[0x50, 0x4B, 0x03, 0x04],
            header_offset: 0,
            footer: Some(&[0x50, 0x4B, 0x05, 0x06]),
            max_size: 100 * 1024 * 1024,
            fixed_size: 0,
        },
        // ── Archives ──────────────────────────────────────────────────────
        FileSignature {
            extension: "zip",
            mime_type: "application/zip",
            header: &[0x50, 0x4B, 0x03, 0x04],
            header_offset: 0,
            footer: Some(&[0x50, 0x4B, 0x05, 0x06]),
            max_size: 4 * 1024 * 1024 * 1024, // 4 GB
            fixed_size: 0,
        },
        FileSignature {
            extension: "rar",
            mime_type: "application/x-rar-compressed",
            header: &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07],
            header_offset: 0,
            footer: None,
            max_size: 4 * 1024 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "7z",
            mime_type: "application/x-7z-compressed",
            header: &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C],
            header_offset: 0,
            footer: None,
            max_size: 4 * 1024 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "gz",
            mime_type: "application/gzip",
            header: &[0x1F, 0x8B],
            header_offset: 0,
            footer: None,
            max_size: 2 * 1024 * 1024 * 1024,
            fixed_size: 0,
        },
        // ── Audio ─────────────────────────────────────────────────────────
        FileSignature {
            extension: "mp3",
            mime_type: "audio/mpeg",
            header: &[0xFF, 0xFB],
            header_offset: 0,
            footer: None,
            max_size: 200 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "mp3",
            mime_type: "audio/mpeg",
            header: b"ID3",
            header_offset: 0,
            footer: None,
            max_size: 200 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "wav",
            mime_type: "audio/wav",
            header: b"RIFF",
            header_offset: 0,
            footer: None,
            max_size: 2 * 1024 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "flac",
            mime_type: "audio/flac",
            header: b"fLaC",
            header_offset: 0,
            footer: None,
            max_size: 500 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "ogg",
            mime_type: "audio/ogg",
            header: b"OggS",
            header_offset: 0,
            footer: None,
            max_size: 500 * 1024 * 1024,
            fixed_size: 0,
        },
        // ── Video ─────────────────────────────────────────────────────────
        FileSignature {
            extension: "mp4",
            mime_type: "video/mp4",
            header: b"ftyp",
            header_offset: 4,
            footer: None,
            max_size: 10 * 1024 * 1024 * 1024, // 10 GB
            fixed_size: 0,
        },
        FileSignature {
            extension: "avi",
            mime_type: "video/x-msvideo",
            header: b"RIFF",
            header_offset: 0,
            footer: None,
            max_size: 10 * 1024 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "mov",
            mime_type: "video/quicktime",
            header: b"ftyp",
            header_offset: 4,
            footer: None,
            max_size: 10 * 1024 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "mkv",
            mime_type: "video/x-matroska",
            header: &[0x1A, 0x45, 0xDF, 0xA3],
            header_offset: 0,
            footer: None,
            max_size: 50 * 1024 * 1024 * 1024,
            fixed_size: 0,
        },
        // ── Databases ─────────────────────────────────────────────────────
        FileSignature {
            extension: "sqlite",
            mime_type: "application/x-sqlite3",
            header: b"SQLite format 3\x00",
            header_offset: 0,
            footer: None,
            max_size: 10 * 1024 * 1024 * 1024,
            fixed_size: 0,
        },
        // ── Other ─────────────────────────────────────────────────────────
        FileSignature {
            extension: "xml",
            mime_type: "application/xml",
            header: b"<?xml",
            header_offset: 0,
            footer: None,
            max_size: 100 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "html",
            mime_type: "text/html",
            header: b"<!DOCTYPE html",
            header_offset: 0,
            footer: None,
            max_size: 50 * 1024 * 1024,
            fixed_size: 0,
        },
        FileSignature {
            extension: "html",
            mime_type: "text/html",
            header: b"<html",
            header_offset: 0,
            footer: None,
            max_size: 50 * 1024 * 1024,
            fixed_size: 0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use recoverx_storage::disk_image::DiskImageSource;

    #[test]
    fn carves_jpeg_from_raw_data() {
        // Construct a minimal "disk" with a JPEG embedded at offset 512
        let mut data = vec![0u8; 4096];
        let offset = 512usize;
        data[offset] = 0xFF;
        data[offset + 1] = 0xD8;
        data[offset + 2] = 0xFF;
        // Add footer at 1024
        data[1024] = 0xFF;
        data[1025] = 0xD9;

        let mut src = DiskImageSource::from_bytes(data, "test.img".into());
        let carver = FileCarver::new();
        let files = carver.carve(&mut src, "test-session", 4096).unwrap();

        assert!(!files.is_empty(), "Should find at least one JPEG");
        let jpeg = files.iter().find(|f| f.extension.as_deref() == Some("jpg"));
        assert!(jpeg.is_some(), "Should have found a jpg");
        assert_eq!(jpeg.unwrap().source_offset, 512);
    }

    #[test]
    fn carves_png_with_correct_footer() {
        let mut data = vec![0u8; 8192];
        let offset = 0usize;
        // PNG header
        data[0..8].copy_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        // PNG footer IEND chunk
        let footer_pos = 4096usize;
        data[footer_pos..footer_pos+8].copy_from_slice(&[0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]);

        let mut src = DiskImageSource::from_bytes(data, "png.img".into());
        let carver = FileCarver::new();
        let files = carver.carve(&mut src, "test", 8192).unwrap();

        let png = files.iter().find(|f| f.extension.as_deref() == Some("png"));
        assert!(png.is_some());
        assert_eq!(png.unwrap().status, FileStatus::Complete);
    }
}
