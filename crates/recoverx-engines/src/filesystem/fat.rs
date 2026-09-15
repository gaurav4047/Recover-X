//! FAT32 / FAT16 filesystem analysis and deleted file recovery.
//!
//! Scans directory entries for records marked as deleted (first byte 0xE5)
//! and reconstructs their metadata. Cluster chains are followed where the
//! FAT chain is intact; otherwise a contiguous-allocation assumption is used.

use std::collections::HashSet;

use recoverx_core::error::Result;
use recoverx_storage::source::StorageSource;

use crate::models::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};
use uuid::Uuid;

pub struct Fat32Analyzer;

struct Bpb {
    bytes_per_sector: u64,
    sectors_per_cluster: u64,
    reserved_sectors: u64,
    num_fats: u64,
    fat_size_32: u64,
    root_cluster: u32,
    #[allow(dead_code)]
    total_sectors: u64,
}

impl Fat32Analyzer {
    /// Analyse a FAT32 volume at `volume_offset` and return all recoverable files.
    pub fn analyze(
        source: &mut dyn StorageSource,
        volume_offset: u64,
        session_id: &str,
        max_files: usize,
    ) -> Result<Vec<RecoveredFile>> {
        let bpb = match Self::read_bpb(source, volume_offset) {
            Ok(b) if b.bytes_per_sector > 0 && b.sectors_per_cluster > 0 => b,
            _ => return Ok(vec![]),
        };

        let cluster_size = bpb.bytes_per_sector * bpb.sectors_per_cluster;
        let fat_offset =
            volume_offset + bpb.reserved_sectors * bpb.bytes_per_sector;
        let fat_size_bytes = bpb.fat_size_32 * bpb.bytes_per_sector;
        let data_start = fat_offset + bpb.num_fats * fat_size_bytes;

        // Read FAT table (capped at 4 MiB)
        let fat_read = (fat_size_bytes as usize).min(4 * 1024 * 1024);
        let mut fat_data = Vec::new();
        source.read(fat_offset, fat_read, &mut fat_data)?;

        let cluster_to_offset = |cluster: u32| -> u64 {
            data_start + (cluster as u64 - 2) * cluster_size
        };

        let mut results = Vec::new();
        let mut visited: HashSet<u32> = HashSet::new();

        Self::scan_dir(
            source,
            &fat_data,
            bpb.root_cluster,
            cluster_size,
            &cluster_to_offset,
            volume_offset,
            session_id,
            &mut results,
            &mut visited,
            max_files,
        )?;

        Ok(results)
    }

    fn read_bpb(source: &mut dyn StorageSource, offset: u64) -> Result<Bpb> {
        let mut buf = Vec::new();
        source.read(offset, 512, &mut buf)?;
        if buf.len() < 90 {
            return Ok(Bpb {
                bytes_per_sector: 0,
                sectors_per_cluster: 0,
                reserved_sectors: 0,
                num_fats: 0,
                fat_size_32: 0,
                root_cluster: 0,
                total_sectors: 0,
            });
        }

        let bps = u16::from_le_bytes([buf[11], buf[12]]) as u64;
        let spc = buf[13] as u64;
        let reserved = u16::from_le_bytes([buf[14], buf[15]]) as u64;
        let num_fats = buf[16] as u64;
        let fat_size_16 = u16::from_le_bytes([buf[22], buf[23]]) as u64;
        let fat_size_32 = u32::from_le_bytes([buf[36], buf[37], buf[38], buf[39]]) as u64;
        let fat_size = if fat_size_32 > 0 {
            fat_size_32
        } else {
            fat_size_16
        };
        let root_cluster = u32::from_le_bytes([buf[44], buf[45], buf[46], buf[47]]);
        let total_16 = u16::from_le_bytes([buf[19], buf[20]]) as u64;
        let total_32 = u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]) as u64;
        let total = if total_32 > 0 { total_32 } else { total_16 };

        Ok(Bpb {
            bytes_per_sector: bps,
            sectors_per_cluster: spc,
            reserved_sectors: reserved,
            num_fats,
            fat_size_32: fat_size,
            root_cluster,
            total_sectors: total,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_dir(
        source: &mut dyn StorageSource,
        fat: &[u8],
        start_cluster: u32,
        cluster_size: u64,
        cluster_to_offset: &dyn Fn(u32) -> u64,
        volume_offset: u64,
        session_id: &str,
        results: &mut Vec<RecoveredFile>,
        visited: &mut HashSet<u32>,
        max_files: usize,
    ) -> Result<()> {
        let mut cluster = start_cluster;

        while cluster >= 2 && cluster < 0x0FFF_FFF8 {
            if !visited.insert(cluster) {
                break;
            }
            if results.len() >= max_files {
                break;
            }

            let dir_offset = cluster_to_offset(cluster);
            let mut dir_data = Vec::new();
            source.read(dir_offset, cluster_size as usize, &mut dir_data)?;

            for chunk in dir_data.chunks_exact(32) {
                if results.len() >= max_files {
                    break;
                }
                if chunk[0] == 0x00 {
                    break; // end of directory
                }
                if chunk[11] == 0x0F {
                    continue; // LFN entry
                }

                let first_byte = chunk[0];
                let attrs = chunk[11];
                let is_deleted = first_byte == 0xE5;
                let is_dir = attrs & 0x10 != 0;
                let is_vol = attrs & 0x08 != 0;

                if is_vol {
                    continue;
                }

                let start_hi =
                    u16::from_le_bytes([chunk[20], chunk[21]]) as u32;
                let start_lo =
                    u16::from_le_bytes([chunk[26], chunk[27]]) as u32;
                let file_cluster = (start_hi << 16) | start_lo;
                let file_size =
                    u32::from_le_bytes([chunk[28], chunk[29], chunk[30], chunk[31]]) as u64;

                if is_deleted && !is_dir && file_size > 0 {
                    // Reconstruct name (first char was overwritten with 0xE5)
                    let mut name_raw = [b'_'; 8];
                    name_raw[1..8].copy_from_slice(&chunk[1..8]);
                    let ext_raw = &chunk[8..11];
                    let name = build_83_name(&name_raw, ext_raw);
                    let ext = ext_str(ext_raw);

                    let data_offset = if file_cluster >= 2 {
                        cluster_to_offset(file_cluster)
                    } else {
                        volume_offset
                    };

                    results.push(RecoveredFile {
                        id: Uuid::new_v4(),
                        session_id: session_id.to_string(),
                        name: name.clone(),
                        original_path: Some(name),
                        size_bytes: file_size,
                        source_offset: data_offset,
                        partition_index: None,
                        filesystem_type: Some("FAT32".into()),
                        recovery_method: RecoveryMethod::DirectoryEntry,
                        confidence: if file_cluster >= 2 { 75 } else { 40 },
                        status: FileStatus::Complete,
                        sha256: None,
                        created_at: None,
                        modified_at: None,
                        accessed_at: None,
                        extension: if ext.is_empty() { None } else { Some(ext) },
                        mime_type: None,
                        is_deleted: true,
                        is_fragmented: false,
                        fragments: vec![FileFragment {
                            offset: data_offset,
                            length: file_size,
                            order: 0,
                        }],
                        metadata: serde_json::json!({
                            "start_cluster": file_cluster,
                            "fat32": true,
                        }),
                    });
                }

                // Recurse into live sub-directories
                if is_dir && !is_deleted && file_cluster >= 2 {
                    let _ = Self::scan_dir(
                        source,
                        fat,
                        file_cluster,
                        cluster_size,
                        cluster_to_offset,
                        volume_offset,
                        session_id,
                        results,
                        visited,
                        max_files,
                    );
                }
            }

            // Follow FAT chain
            let next = fat_next_cluster(fat, cluster);
            if next >= 0x0FFF_FFF8 || next < 2 {
                break;
            }
            cluster = next;
        }

        Ok(())
    }
}

fn fat_next_cluster(fat: &[u8], cluster: u32) -> u32 {
    let off = (cluster as usize) * 4;
    if off + 4 > fat.len() {
        return 0x0FFF_FFFF;
    }
    u32::from_le_bytes(fat[off..off + 4].try_into().unwrap_or([0xFF; 4])) & 0x0FFF_FFFF
}

fn build_83_name(name: &[u8], ext: &[u8]) -> String {
    let n: String = name
        .iter()
        .take_while(|&&b| b != b' ' && b != 0)
        .map(|&b| b as char)
        .collect();
    let e: String = ext
        .iter()
        .take_while(|&&b| b != b' ' && b != 0)
        .map(|&b| b as char)
        .collect();
    if e.is_empty() {
        n
    } else {
        format!("{}.{}", n, e)
    }
}

fn ext_str(ext: &[u8]) -> String {
    ext.iter()
        .take_while(|&&b| b != b' ' && b != 0)
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect()
}
