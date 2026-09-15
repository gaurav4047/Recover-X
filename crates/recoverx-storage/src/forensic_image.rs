//! Forensic image source (E01 / AFF4).
//!
//! EWF/E01 and AFF4 forensic image formats are not currently supported.
//! `ForensicImageSource::open()` always returns `UnsupportedImageFormat`.
//! Use `DiskImageSource` for RAW/DD/IMG files.

use recoverx_core::error::{RecoverXError, Result};

use crate::source::StorageSource;

/// Placeholder for E01/AFF4 forensic image support.
pub struct ForensicImageSource;

impl ForensicImageSource {
    /// Always returns an error — forensic image parsing is not yet implemented.
    pub fn open(path: &str) -> Result<Self> {
        Err(RecoverXError::UnsupportedImageFormat {
            format: format!(
                "{} — E01/AFF4 forensic image support is not yet implemented. Use a RAW/DD/IMG image instead.",
                path
            ),
        })
    }
}

impl StorageSource for ForensicImageSource {
    fn read(&mut self, _offset: u64, _length: usize, _buf: &mut Vec<u8>) -> Result<usize> {
        Err(RecoverXError::UnsupportedImageFormat {
            format: "ForensicImageSource: E01/AFF4 not implemented".to_string(),
        })
    }

    fn size(&self) -> u64 { 0 }
    fn sector_size(&self) -> u32 { 512 }
    fn metadata(&self) -> &crate::source::StorageSourceMetadata {
        unimplemented!("ForensicImageSource metadata not implemented")
    }
}
