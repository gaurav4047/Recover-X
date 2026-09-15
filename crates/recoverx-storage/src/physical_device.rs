//! Physical block device source.
//!
//! Opens the device in read-only mode using the OS file I/O layer.
//! On macOS/Linux this means opening `/dev/diskN` or `/dev/sdX` with O_RDONLY.
//! On Windows it means opening `\\.\PhysicalDriveN` with GENERIC_READ.
//!
//! Phase 1: model and serial number detection is a best-effort operation.
//! If elevated privileges are required and not available, an explicit
//! `InsufficientPrivileges` error is returned.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use recoverx_core::error::{RecoverXError, Result};

use crate::source::{StorageSource, StorageSourceMetadata, StorageSourceType};

/// A read-only handle to a physical block device.
pub struct PhysicalDeviceSource {
    file: File,
    metadata: StorageSourceMetadata,
}

impl PhysicalDeviceSource {
    /// Open a physical device at `path` in read-only mode.
    ///
    /// Returns `InsufficientPrivileges` if the OS rejects the open.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let file = OpenOptions::new()
            .read(true)
            .write(false) // NEVER write
            .open(&path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    RecoverXError::InsufficientPrivileges {
                        resource: path.display().to_string(),
                        detail: "Run with elevated privileges (sudo / Administrator) to access raw devices".to_string(),
                    }
                } else if e.kind() == std::io::ErrorKind::NotFound {
                    RecoverXError::DeviceNotFound {
                        path: path.display().to_string(),
                    }
                } else {
                    RecoverXError::Io(e)
                }
            })?;

        let size_bytes = detect_size(&file, &path)?;
        let sector_size = detect_sector_size(&path);

        let metadata = StorageSourceMetadata {
            label: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
            path: path.clone(),
            size_bytes,
            sector_size,
            source_type: StorageSourceType::PhysicalDevice,
            model: None,  // TODO: ioctl / WMI
            serial: None, // TODO: ioctl / WMI
            filesystem_type: None,
            filesystem_label: None,
            is_read_only: true,
        };

        Ok(PhysicalDeviceSource { file, metadata })
    }
}

impl StorageSource for PhysicalDeviceSource {
    fn read(&mut self, offset: u64, length: usize, buf: &mut Vec<u8>) -> Result<usize> {
        if offset >= self.metadata.size_bytes {
            return Err(RecoverXError::ReadOutOfBounds {
                offset,
                length,
                source_size: self.metadata.size_bytes,
            });
        }

        self.file.seek(SeekFrom::Start(offset))?;

        let capacity = length.min((self.metadata.size_bytes - offset) as usize);
        buf.resize(capacity, 0u8);

        let n = self.file.read(buf)?;
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

// ── platform helpers ──────────────────────────────────────────────────────────

/// Detect the size of a block device.
/// For regular files this falls back to file metadata.
fn detect_size(file: &File, path: &Path) -> Result<u64> {
    // Try seeking to end — works for image files and some block devices
    use std::io::Seek;
    let mut f = file.try_clone()?;
    match f.seek(SeekFrom::End(0)) {
        Ok(size) if size > 0 => return Ok(size),
        _ => {}
    }

    // Try file metadata
    match file.metadata() {
        Ok(meta) if meta.len() > 0 => return Ok(meta.len()),
        _ => {}
    }

    // Platform-specific block device size detection
    #[cfg(target_os = "macos")]
    {
        if let Ok(size) = macos_block_device_size(file) {
            return Ok(size);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(size) = linux_block_device_size(file) {
            return Ok(size);
        }
    }

    Err(RecoverXError::SourceNotReadable {
        reason: format!("Cannot determine size of {}", path.display()),
    })
}

fn detect_sector_size(_path: &Path) -> u32 {
    // Default to 512; TODO: use ioctl to detect 4Kn drives
    512
}

#[cfg(target_os = "macos")]
fn macos_block_device_size(file: &File) -> std::io::Result<u64> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();

    // DKIOCGETBLOCKCOUNT * DKIOCGETBLOCKSIZE
    const DKIOCGETBLOCKCOUNT: u64 = 0x40086419;
    const DKIOCGETBLOCKSIZE: u64 = 0x40046418;

    let mut block_count: u64 = 0;
    let mut block_size: u32 = 0;

    let ret_count = unsafe { libc::ioctl(fd, DKIOCGETBLOCKCOUNT, &mut block_count as *mut u64) };
    let ret_size = unsafe { libc::ioctl(fd, DKIOCGETBLOCKSIZE, &mut block_size as *mut u32) };

    if ret_count == 0 && ret_size == 0 && block_count > 0 && block_size > 0 {
        Ok(block_count * block_size as u64)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn linux_block_device_size(file: &File) -> std::io::Result<u64> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();

    const BLKGETSIZE64: u64 = 0x80081272;
    let mut size: u64 = 0;

    let ret = unsafe { libc::ioctl(fd, BLKGETSIZE64, &mut size as *mut u64) };
    if ret == 0 && size > 0 {
        Ok(size)
    } else {
        Err(std::io::Error::last_os_error())
    }
}
