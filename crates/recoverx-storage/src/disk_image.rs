//! Disk image source (RAW / DD / IMG files).
//!
//! Reads a flat binary image file using standard file I/O.
//! The same filesystem and carving engines that operate on physical devices
//! will work unchanged on disk images via this implementation.

use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use hex;
use recoverx_core::error::{RecoverXError, Result};
use sha2::{Digest, Sha256};

use crate::source::{StorageSource, StorageSourceMetadata, StorageSourceType};

enum ImageBackend {
    File(File),
    Memory(Cursor<Vec<u8>>),
}

/// A read-only disk image source backed by a file or in-memory buffer.
pub struct DiskImageSource {
    backend: ImageBackend,
    metadata: StorageSourceMetadata,
    /// SHA-256 of the first 64 KiB (computed on open).
    pub partial_sha256: Option<String>,
}

impl DiskImageSource {
    /// Open a flat binary image file (RAW / DD / IMG).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Validate extension (E01 is not supported; use RAW/DD/IMG)
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
        match ext.as_deref() {
            Some("img") | Some("dd") | Some("raw") | Some("bin") | None => {}
            Some(other) => {
                // Allow but warn about unrecognised extensions
                tracing::warn!(
                    "DiskImageSource: unrecognised extension '{}' for {}",
                    other,
                    path.display()
                );
            }
        }

        let file = OpenOptions::new()
            .read(true)
            .write(false)
            .open(&path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    RecoverXError::InsufficientPrivileges {
                        resource: path.display().to_string(),
                        detail: "Cannot read image file".to_string(),
                    }
                } else if e.kind() == std::io::ErrorKind::NotFound {
                    RecoverXError::ImageNotFound {
                        path: path.display().to_string(),
                    }
                } else {
                    RecoverXError::Io(e)
                }
            })?;

        let size_bytes = file.metadata()?.len();
        if size_bytes == 0 {
            return Err(RecoverXError::SourceNotReadable {
                reason: format!("Image file is empty: {}", path.display()),
            });
        }

        let partial_sha256 = compute_partial_hash(&file)?;

        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let metadata = StorageSourceMetadata {
            label,
            path: path.clone(),
            size_bytes,
            sector_size: 512,
            source_type: StorageSourceType::DiskImage,
            model: None,
            serial: None,
            filesystem_type: None,
            filesystem_label: None,
            is_read_only: true,
        };

        Ok(DiskImageSource {
            backend: ImageBackend::File(file),
            metadata,
            partial_sha256: Some(partial_sha256),
        })
    }

    /// Construct an in-memory image source (used for testing).
    pub fn from_bytes(data: Vec<u8>, label: String) -> Self {
        let size_bytes = data.len() as u64;
        let metadata = StorageSourceMetadata {
            label: label.clone(),
            path: std::path::PathBuf::from(label),
            size_bytes,
            sector_size: 512,
            source_type: StorageSourceType::DiskImage,
            model: None,
            serial: None,
            filesystem_type: None,
            filesystem_label: None,
            is_read_only: true,
        };
        DiskImageSource {
            backend: ImageBackend::Memory(Cursor::new(data)),
            metadata,
            partial_sha256: None,
        }
    }

    /// Return a reference to the in-memory bytes (test helper).
    /// Returns an empty slice if the backend is file-based.
    pub fn as_bytes(&self) -> &[u8] {
        match &self.backend {
            ImageBackend::Memory(cursor) => cursor.get_ref().as_slice(),
            ImageBackend::File(_) => &[],
        }
    }
}

impl StorageSource for DiskImageSource {
    fn read(&mut self, offset: u64, length: usize, buf: &mut Vec<u8>) -> Result<usize> {
        if offset >= self.metadata.size_bytes {
            return Err(RecoverXError::ReadOutOfBounds {
                offset,
                length,
                source_size: self.metadata.size_bytes,
            });
        }

        let clamped = length.min((self.metadata.size_bytes - offset) as usize);
        buf.resize(clamped, 0u8);

        let n = match &mut self.backend {
            ImageBackend::File(f) => {
                f.seek(SeekFrom::Start(offset))?;
                f.read(buf)?
            }
            ImageBackend::Memory(cursor) => {
                cursor.seek(SeekFrom::Start(offset))?;
                cursor.read(buf)?
            }
        };

        buf.truncate(n);
        Ok(n)
    }

    fn size(&self) -> u64 {
        self.metadata.size_bytes
    }

    fn sector_size(&self) -> u32 {
        self.metadata.sector_size
    }

    fn metadata(&self) -> &StorageSourceMetadata {
        &self.metadata
    }
}

fn compute_partial_hash(file: &File) -> Result<String> {
    const PARTIAL_SIZE: usize = 65536; // 64 KiB
    let mut f = file.try_clone()?;
    f.seek(SeekFrom::Start(0))?;

    let size = f.metadata()?.len().min(PARTIAL_SIZE as u64) as usize;
    let mut buf = vec![0u8; size];
    f.read_exact(&mut buf)?;

    let mut hasher = Sha256::new();
    hasher.update(&buf);
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_in_memory_image() {
        let data: Vec<u8> = (0u8..=255u8).cycle().take(1024).collect();
        let mut src = DiskImageSource::from_bytes(data.clone(), "test.img".to_string());
        assert_eq!(src.size(), 1024);

        let mut buf = Vec::new();
        let n = src.read(0, 256, &mut buf).unwrap();
        assert_eq!(n, 256);
        assert_eq!(buf, &data[..256]);
    }

    #[test]
    fn read_at_offset() {
        let data: Vec<u8> = (0u8..=255u8).cycle().take(1024).collect();
        let mut src = DiskImageSource::from_bytes(data.clone(), "test.img".to_string());

        let mut buf = Vec::new();
        let n = src.read(512, 256, &mut buf).unwrap();
        assert_eq!(n, 256);
        assert_eq!(buf, &data[512..768]);
    }

    #[test]
    fn read_out_of_bounds_returns_error() {
        let data = vec![0u8; 512];
        let mut src = DiskImageSource::from_bytes(data, "test.img".to_string());
        let mut buf = Vec::new();
        assert!(src.read(512, 1, &mut buf).is_err());
    }

    #[test]
    fn read_clamps_at_end() {
        let data = vec![0xFFu8; 100];
        let mut src = DiskImageSource::from_bytes(data, "test.img".to_string());
        let mut buf = Vec::new();
        // Request 200 bytes from offset 50 — should only get 50
        let n = src.read(50, 200, &mut buf).unwrap();
        assert_eq!(n, 50);
    }

    #[test]
    fn read_sector() {
        let mut data = vec![0u8; 1024];
        for b in &mut data[512..1024] {
            *b = 0xFF;
        }
        let mut src = DiskImageSource::from_bytes(data, "test.img".to_string());

        let mut buf = Vec::new();
        src.read_sector(1, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0xFF));
    }
}
