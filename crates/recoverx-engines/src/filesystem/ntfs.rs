//! NTFS filesystem analysis — MFT record scanning for deleted files.
//!
//! Scans the Master File Table for file records marked as not-in-use (deleted).
//! The MFT is located via the NTFS BPB at volume start.

use recoverx_core::error::Result;
use recoverx_storage::source::StorageSource;
use uuid::Uuid;

use crate::models::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};

pub struct NtfsAnalyzer;

const MFT_RECORD_SIZE: usize = 1024;
const MFT_MAGIC: &[u8] = b"FILE";

impl NtfsAnalyzer {
    /// Analyse an NTFS volume at `volume_offset` and return all recoverable files.
    pub fn analyze(
        source: &mut dyn StorageSource,
        volume_offset: u64,
        session_id: &str,
        max_files: usize,
    ) -> Result<Vec<RecoveredFile>> {
        // Read BPB to locate MFT
        let mut boot = Vec::new();
        if source.read(volume_offset, 512, &mut boot)? < 512 {
            return Ok(vec![]);
        }

        if boot.len() < 11 || &boot[3..7] != b"NTFS" {
            return Ok(vec![]);
        }

        let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]) as u64;
        let sectors_per_cluster = boot[13] as u64;
        if bytes_per_sector == 0 || sectors_per_cluster == 0 {
            return Ok(vec![]);
        }
        let cluster_size = bytes_per_sector * sectors_per_cluster;

        // MFT start LCN at offset 48 (8 bytes)
        let mft_lcn = u64::from_le_bytes(
            boot[48..56].try_into().unwrap_or([0; 8])
        );
        let mft_offset = volume_offset + mft_lcn * cluster_size;

        // Clusters per MFT record (signed byte at offset 64)
        let clusters_per_record = boot[64] as i8;
        let record_size = if clusters_per_record > 0 {
            (clusters_per_record as u64) * cluster_size
        } else {
            // Negative means 2^|value| bytes
            1u64 << ((-clusters_per_record) as u64)
        };
        let record_size = if record_size < 512 { MFT_RECORD_SIZE as u64 } else { record_size };

        // Read MFT in chunks; scan for deleted FILE records
        let mut results = Vec::new();
        let chunk_records = 64usize;
        let chunk_size = record_size as usize * chunk_records;
        let mut mft_pos = mft_offset;
        let source_end = source.size();
        let mut record_index: u32 = 0;

        while mft_pos + record_size <= source_end && results.len() < max_files {
            let read_size = chunk_size.min((source_end - mft_pos) as usize);
            let mut chunk = Vec::new();
            match source.read(mft_pos, read_size, &mut chunk) {
                Ok(n) if n >= record_size as usize => {}
                _ => break,
            }

            for rec_start in (0..chunk.len()).step_by(record_size as usize) {
                if results.len() >= max_files { break; }
                let rec_end = (rec_start + record_size as usize).min(chunk.len());
                let rec = &chunk[rec_start..rec_end];

                if rec.len() < 48 { record_index += 1; continue; }

                // Must start with "FILE" magic
                if &rec[0..4] != MFT_MAGIC { record_index += 1; continue; }

                // Flags at offset 22: bit 0 = in use, bit 1 = directory
                let flags = u16::from_le_bytes([rec[22], rec[23]]);
                let in_use = flags & 0x01 != 0;
                let is_dir = flags & 0x02 != 0;

                // Only care about deleted non-directory files
                if in_use || is_dir { record_index += 1; continue; }

                // Parse attributes to get name and data size
                if let Some((name, size, data_offset)) = parse_mft_record(rec, mft_pos + rec_start as u64, volume_offset, cluster_size) {
                    if size == 0 { record_index += 1; continue; }

                    let ext = extension_from_name(&name);
                    results.push(RecoveredFile {
                        id: Uuid::new_v4(),
                        session_id: session_id.to_string(),
                        name: name.clone(),
                        original_path: Some(name),
                        size_bytes: size,
                        source_offset: data_offset,
                        partition_index: None,
                        filesystem_type: Some("NTFS".into()),
                        recovery_method: RecoveryMethod::MftRecord,
                        confidence: 70,
                        status: FileStatus::Complete,
                        sha256: None,
                        created_at: None,
                        modified_at: None,
                        accessed_at: None,
                        extension: ext.clone(),
                        mime_type: None,
                        is_deleted: true,
                        is_fragmented: false,
                        fragments: vec![FileFragment {
                            offset: data_offset,
                            length: size,
                            order: 0,
                        }],
                        metadata: serde_json::json!({
                            "ntfs": true,
                            "record_index": record_index,
                        }),
                    });
                }

                record_index += 1;
            }

            mft_pos += chunk_size as u64;
        }

        Ok(results)
    }
}

/// Parse a single MFT record, returning (name, data_size, data_offset) or None.
fn parse_mft_record(rec: &[u8], rec_abs_offset: u64, _volume_offset: u64, _cluster_size: u64) -> Option<(String, u64, u64)> {
    if rec.len() < 48 { return None; }

    // First attribute offset at bytes 20-21
    let first_attr = u16::from_le_bytes([rec[20], rec[21]]) as usize;
    if first_attr >= rec.len() { return None; }

    let mut name = String::new();
    let mut data_size: u64 = 0;
    let mut data_offset: u64 = 0;

    let mut pos = first_attr;
    while pos + 4 <= rec.len() {
        let attr_type = u32::from_le_bytes([rec[pos], rec[pos+1], rec[pos+2], rec[pos+3]]);
        if attr_type == 0xFFFFFFFF { break; } // end marker

        if pos + 8 > rec.len() { break; }
        let attr_len = u32::from_le_bytes([rec[pos+4], rec[pos+5], rec[pos+6], rec[pos+7]]) as usize;
        if attr_len == 0 || pos + attr_len > rec.len() { break; }

        let non_resident = if pos + 9 <= rec.len() { rec[pos + 8] } else { 0 };

        match attr_type {
            // $FILE_NAME (0x30)
            0x30 if name.is_empty() => {
                if non_resident == 0 && pos + 24 <= rec.len() {
                    let value_offset = u16::from_le_bytes([rec[pos+20], rec[pos+21]]) as usize;
                    let _value_len = u32::from_le_bytes([rec[pos+16], rec[pos+17], rec[pos+18], rec[pos+19]]) as usize;
                    let val_start = pos + value_offset;
                    // $FILE_NAME attribute: parent_ref(8) + timestamps(32) + sizes(16) + flags(4) + name_len(1) + ns(1) + name(2*n)
                    if val_start + 66 <= rec.len() {
                        let name_len = rec[val_start + 64] as usize;
                        let name_offset = val_start + 66;
                        if name_offset + name_len * 2 <= rec.len() {
                            let name_bytes = &rec[name_offset..name_offset + name_len * 2];
                            let chars: Vec<u16> = name_bytes.chunks_exact(2)
                                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                                .collect();
                            name = String::from_utf16_lossy(&chars).to_string();
                        }
                    }
                }
            }
            // $DATA (0x80)
            0x80 if data_size == 0 => {
                if non_resident == 0 {
                    // Resident: value is inline
                    if pos + 20 <= rec.len() {
                        let value_len = u32::from_le_bytes([rec[pos+16], rec[pos+17], rec[pos+18], rec[pos+19]]) as u64;
                        let value_offset = u16::from_le_bytes([rec[pos+20], rec[pos+21]]) as usize;
                        data_size = value_len;
                        data_offset = rec_abs_offset + (pos + value_offset) as u64;
                    }
                } else {
                    // Non-resident: size at offset 48 relative to attr start
                    if pos + 56 <= rec.len() {
                        data_size = u64::from_le_bytes(rec[pos+48..pos+56].try_into().unwrap_or([0;8]));
                        // Real data offset would require parsing data runs; use rec offset as hint
                        data_offset = rec_abs_offset;
                    }
                }
            }
            _ => {}
        }

        pos += attr_len;
    }

    if name.is_empty() || data_size == 0 {
        return None;
    }

    Some((name, data_size, data_offset))
}

fn extension_from_name(name: &str) -> Option<String> {
    name.rsplit('.').next()
        .filter(|e| !e.is_empty() && *e != name)
        .map(|e| e.to_lowercase())
}
