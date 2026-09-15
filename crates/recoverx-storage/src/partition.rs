//! A partition is a byte-range window on top of another `StorageSource`.
//!
//! This lets the recovery engines operate on a single partition without
//! knowing whether it lives on a physical device or a disk image.

use crate::source::{StorageSource, StorageSourceMetadata, StorageSourceType};
use recoverx_core::error::{RecoverXError, Result};

/// A read-only view of a partition within a parent source.
pub struct PartitionSource<S: StorageSource> {
    parent: S,
    /// Byte offset of the partition start within the parent source.
    start_offset: u64,
    /// Size of the partition in bytes.
    partition_size: u64,
    metadata: StorageSourceMetadata,
}

impl<S: StorageSource> PartitionSource<S> {
    /// Create a partition view.
    ///
    /// # Parameters
    /// - `parent`         — the underlying `StorageSource`
    /// - `start_offset`   — byte offset of the partition start within the parent
    /// - `partition_size` — size of the partition in bytes
    /// - `index`          — partition index (for labelling)
    pub fn new(
        parent: S,
        start_offset: u64,
        partition_size: u64,
        index: u32,
        partition_type: Option<String>,
        filesystem_type: Option<String>,
    ) -> Result<Self> {
        let parent_size = parent.size();
        if start_offset >= parent_size {
            return Err(RecoverXError::ReadOutOfBounds {
                offset: start_offset,
                length: 0,
                source_size: parent_size,
            });
        }
        if start_offset + partition_size > parent_size {
            return Err(RecoverXError::ReadOutOfBounds {
                offset: start_offset,
                length: partition_size as usize,
                source_size: parent_size,
            });
        }

        let parent_meta = parent.metadata();
        let label = format!(
            "{} — partition {} ({})",
            parent_meta.label,
            index,
            partition_type.as_deref().unwrap_or("unknown")
        );
        let path = parent_meta.path.clone();
        let sector_size = parent_meta.sector_size;

        let metadata = StorageSourceMetadata {
            label,
            path,
            size_bytes: partition_size,
            sector_size,
            source_type: StorageSourceType::Partition,
            model: parent_meta.model.clone(),
            serial: parent_meta.serial.clone(),
            filesystem_type,
            filesystem_label: None,
            is_read_only: true,
        };

        Ok(PartitionSource {
            parent,
            start_offset,
            partition_size,
            metadata,
        })
    }

    /// Return the byte offset of this partition within its parent.
    pub fn start_offset(&self) -> u64 {
        self.start_offset
    }
}

impl<S: StorageSource> StorageSource for PartitionSource<S> {
    fn read(&mut self, offset: u64, length: usize, buf: &mut Vec<u8>) -> Result<usize> {
        if offset >= self.partition_size {
            return Err(RecoverXError::ReadOutOfBounds {
                offset,
                length,
                source_size: self.partition_size,
            });
        }

        let clamped_length = length.min((self.partition_size - offset) as usize);
        let absolute_offset = self.start_offset + offset;

        self.parent.read(absolute_offset, clamped_length, buf)
    }

    fn size(&self) -> u64 {
        self.partition_size
    }

    fn sector_size(&self) -> u32 {
        self.metadata.sector_size
    }

    fn metadata(&self) -> &StorageSourceMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk_image::DiskImageSource;

    /// Create a test image with known byte pattern.
    fn make_test_image() -> Vec<u8> {
        let mut data = vec![0u8; 4096];
        // First 512 bytes (sector 0) = 0xAA
        for b in &mut data[..512] {
            *b = 0xAA;
        }
        // Second 512 bytes (sector 1) = 0xBB
        for b in &mut data[512..1024] {
            *b = 0xBB;
        }
        // Third sector = 0xCC
        for b in &mut data[1024..1536] {
            *b = 0xCC;
        }
        data
    }

    fn disk_image_source(data: Vec<u8>) -> DiskImageSource {
        DiskImageSource::from_bytes(data, "test.img".to_string())
    }

    #[test]
    fn partition_reads_correct_offset() {
        let data = make_test_image();
        let img = disk_image_source(data);
        // Partition starting at sector 1 (offset 512), size 512
        let mut partition = PartitionSource::new(img, 512, 512, 0, None, None).unwrap();
        let mut buf = Vec::new();
        let n = partition.read(0, 512, &mut buf).unwrap();
        assert_eq!(n, 512);
        assert!(buf.iter().all(|&b| b == 0xBB));
    }

    #[test]
    fn partition_rejects_out_of_bounds() {
        let data = make_test_image();
        let img = disk_image_source(data);
        let mut partition = PartitionSource::new(img, 512, 512, 0, None, None).unwrap();
        let mut buf = Vec::new();
        // Trying to read at offset == partition size
        assert!(partition.read(512, 1, &mut buf).is_err());
    }

    #[test]
    fn partition_creation_rejects_overflow() {
        let data = make_test_image(); // 4096 bytes
        let img = disk_image_source(data);
        // start=3600, size=512 → end=4112 > 4096
        let result = PartitionSource::new(img, 3600, 512, 0, None, None);
        assert!(result.is_err());
    }
}
