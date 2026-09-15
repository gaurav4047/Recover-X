//! ext2/3/4 filesystem analysis — inode and directory scanning for deleted files.
//!
//! Reads the ext superblock (at offset 1024 from volume start), then iterates
//! block groups to find deleted inodes (dtime > 0).

use recoverx_core::error::Result;
use recoverx_storage::source::StorageSource;
use uuid::Uuid;

use crate::models::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};

pub struct Ext4Analyzer;

impl Ext4Analyzer {
    /// Analyse an ext2/3/4 volume at `volume_offset` and return recoverable deleted files.
    pub fn analyze(
        source: &mut dyn StorageSource,
        volume_offset: u64,
        session_id: &str,
        max_files: usize,
    ) -> Result<Vec<RecoveredFile>> {
        // Read superblock at offset 1024 from volume start
        let sb_offset = volume_offset + 1024;
        let mut sb = Vec::new();
        if source.read(sb_offset, 1024, &mut sb)? < 256 {
            return Ok(vec![]);
        }

        // Verify ext magic 0xEF53 at offset 56 within superblock
        let magic = u16::from_le_bytes([sb[56], sb[57]]);
        if magic != 0xEF53 {
            return Ok(vec![]);
        }

        let block_count = u32::from_le_bytes(sb[4..8].try_into().unwrap_or([0;4])) as u64;
        let block_size_shift = u32::from_le_bytes(sb[24..28].try_into().unwrap_or([0;4]));
        let block_size = 1024u64 << block_size_shift;

        let inodes_per_group = u32::from_le_bytes(sb[40..44].try_into().unwrap_or([0;4])) as u64;
        let inode_size = u16::from_le_bytes([sb[88], sb[89]]) as u64;
        let inode_size = if inode_size < 128 { 128u64 } else { inode_size };

        let blocks_per_group = u32::from_le_bytes(sb[32..36].try_into().unwrap_or([0;4])) as u64;
        if blocks_per_group == 0 || inodes_per_group == 0 || block_size == 0 {
            return Ok(vec![]);
        }

        let num_groups = (block_count + blocks_per_group - 1) / blocks_per_group;

        // Block group descriptor table starts at block 1 (or block 2 for 1K blocks)
        let bgdt_block = if block_size == 1024 { 2u64 } else { 1u64 };
        let bgdt_offset = volume_offset + bgdt_block * block_size;

        let mut results = Vec::new();
        let descriptor_size = 32usize; // ext2/3 block group descriptor size

        for group in 0..num_groups {
            if results.len() >= max_files { break; }

            let desc_offset = bgdt_offset + group * descriptor_size as u64;
            let mut desc = Vec::new();
            if source.read(desc_offset, descriptor_size, &mut desc)? < descriptor_size {
                continue;
            }

            let inode_table_block = u32::from_le_bytes(desc[8..12].try_into().unwrap_or([0;4])) as u64;
            let inode_table_offset = volume_offset + inode_table_block * block_size;

            // Read inode table for this group
            let table_size = (inodes_per_group * inode_size) as usize;
            let read_size = table_size.min(512 * 1024); // cap at 512 KiB per group
            let mut inode_table = Vec::new();
            if source.read(inode_table_offset, read_size, &mut inode_table)? < inode_size as usize {
                continue;
            }

            for i in 0..(inode_table.len() / inode_size as usize) {
                if results.len() >= max_files { break; }

                let inode_off = i * inode_size as usize;
                if inode_off + 128 > inode_table.len() { break; }

                let inode = &inode_table[inode_off..inode_off + 128.min(inode_size as usize)];

                // mode at offset 0
                let mode = u16::from_le_bytes([inode[0], inode[1]]);
                // Only regular files (0x8000)
                if mode & 0xF000 != 0x8000 { continue; }

                // dtime (deletion time) at offset 20 — nonzero means deleted
                let dtime = u32::from_le_bytes(inode[20..24].try_into().unwrap_or([0;4]));
                if dtime == 0 { continue; }

                // size (lower 32 bits at offset 4, upper at offset 108)
                let size_lo = u32::from_le_bytes(inode[4..8].try_into().unwrap_or([0;4])) as u64;
                let size_hi = u32::from_le_bytes(inode[108..112].try_into().unwrap_or([0;4])) as u64;
                let file_size = size_lo | (size_hi << 32);
                if file_size == 0 { continue; }

                // Block 0 direct pointer at offset 40
                let block0 = u32::from_le_bytes(inode[40..44].try_into().unwrap_or([0;4])) as u64;
                let data_offset = if block0 > 0 { volume_offset + block0 * block_size } else { 0 };

                // Global inode number
                let inode_num = group * inodes_per_group + i as u64 + 1;

                let name = format!("deleted_inode_{}.bin", inode_num);

                results.push(RecoveredFile {
                    id: Uuid::new_v4(),
                    session_id: session_id.to_string(),
                    name: name.clone(),
                    original_path: None, // Would need directory scan to reconstruct
                    size_bytes: file_size,
                    source_offset: data_offset,
                    partition_index: None,
                    filesystem_type: Some("ext4".into()),
                    recovery_method: RecoveryMethod::InodeRecord,
                    confidence: 55, // No filename info
                    status: FileStatus::Complete,
                    sha256: None,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    extension: None,
                    mime_type: None,
                    is_deleted: true,
                    is_fragmented: false,
                    fragments: vec![FileFragment {
                        offset: data_offset,
                        length: file_size,
                        order: 0,
                    }],
                    metadata: serde_json::json!({
                        "ext4": true,
                        "inode_number": inode_num,
                        "dtime": dtime,
                    }),
                });
            }
        }

        Ok(results)
    }
}
