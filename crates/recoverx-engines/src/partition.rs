//! MBR and GPT partition table parsers.
//!
//! Both parsers operate on a `&mut dyn StorageSource` so they work identically
//! on physical devices, partitions, and disk images.

use recoverx_core::error::Result;
use recoverx_storage::source::StorageSource;

use crate::models::{DetectedPartition, PartitionTableType};

pub struct PartitionDetector;

impl PartitionDetector {
    /// Detect and return all partitions from the source.
    /// Tries GPT first (via protective MBR), then falls back to MBR.
    pub fn detect(source: &mut dyn StorageSource) -> Result<Vec<DetectedPartition>> {
        let sector_size = source.sector_size() as u64;

        // Read sector 0
        let mut sector0 = Vec::new();
        let n = source.read(0, 512, &mut sector0)?;
        if n < 512 {
            return Ok(vec![]);
        }

        // Check MBR boot signature
        if sector0[510] != 0x55 || sector0[511] != 0xAA {
            return Ok(vec![]);
        }

        // Check for GPT protective MBR (partition type 0xEE)
        let partition_type_0 = sector0[446 + 4];
        if partition_type_0 == 0xEE {
            match Self::parse_gpt(source, sector_size) {
                Ok(parts) if !parts.is_empty() => return Ok(parts),
                _ => {}
            }
        }

        Self::parse_mbr(&sector0)
    }

    fn parse_mbr(sector0: &[u8]) -> Result<Vec<DetectedPartition>> {
        let mut partitions = Vec::new();

        for i in 0..4usize {
            let offset = 446 + i * 16;
            if offset + 16 > sector0.len() {
                break;
            }
            let entry = &sector0[offset..offset + 16];

            let status = entry[0];
            let part_type = entry[4];

            if part_type == 0 {
                continue; // Empty entry
            }

            let start_lba =
                u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as u64;
            let sector_count =
                u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as u64;

            if start_lba == 0 || sector_count == 0 {
                continue;
            }

            partitions.push(DetectedPartition {
                index: i as u32,
                partition_table_type: PartitionTableType::Mbr,
                type_guid: None,
                type_id: Some(part_type),
                start_lba,
                end_lba: start_lba + sector_count - 1,
                start_offset: start_lba * 512,
                size_bytes: sector_count * 512,
                name: Some(mbr_type_name(part_type).to_string()),
                filesystem: guess_mbr_filesystem(part_type),
                is_bootable: status == 0x80,
                is_active: status == 0x80,
            });
        }

        Ok(partitions)
    }

    fn parse_gpt(
        source: &mut dyn StorageSource,
        sector_size: u64,
    ) -> Result<Vec<DetectedPartition>> {
        // GPT header at LBA 1
        let mut header_buf = Vec::new();
        source.read(sector_size, 512, &mut header_buf)?;

        if header_buf.len() < 92 {
            return Ok(vec![]);
        }

        // Check GPT signature "EFI PART"
        if &header_buf[0..8] != b"EFI PART" {
            return Ok(vec![]);
        }

        let num_entries =
            u32::from_le_bytes([header_buf[80], header_buf[81], header_buf[82], header_buf[83]])
                as usize;
        let entry_size =
            u32::from_le_bytes([header_buf[84], header_buf[85], header_buf[86], header_buf[87]])
                as usize;
        let entries_lba = u64::from_le_bytes(
            header_buf[72..80]
                .try_into()
                .unwrap_or([0, 0, 0, 0, 0, 0, 0, 0]),
        );

        if num_entries == 0 || entry_size < 128 || num_entries > 256 {
            return Ok(vec![]);
        }

        let entries_offset = entries_lba * sector_size;
        let total_size = (num_entries * entry_size).min(1024 * 1024); // cap at 1 MiB
        let mut entries_buf = Vec::new();
        source.read(entries_offset, total_size, &mut entries_buf)?;

        let mut partitions = Vec::new();

        for i in 0..num_entries {
            let base = i * entry_size;
            if base + 128 > entries_buf.len() {
                break;
            }
            let entry = &entries_buf[base..base + 128];

            // All-zero type GUID = empty entry
            if entry[..16].iter().all(|&b| b == 0) {
                continue;
            }

            let type_guid = format_guid(&entry[0..16]);
            let start_lba =
                u64::from_le_bytes(entry[32..40].try_into().unwrap_or([0; 8]));
            let end_lba =
                u64::from_le_bytes(entry[40..48].try_into().unwrap_or([0; 8]));

            if start_lba == 0 || end_lba < start_lba {
                continue;
            }

            let name = decode_utf16le(&entry[56..128]);
            let fs_hint = gpt_type_filesystem(&type_guid);

            partitions.push(DetectedPartition {
                index: i as u32,
                partition_table_type: PartitionTableType::Gpt,
                type_guid: Some(type_guid.clone()),
                type_id: None,
                start_lba,
                end_lba,
                start_offset: start_lba * sector_size,
                size_bytes: (end_lba - start_lba + 1) * sector_size,
                name: if name.is_empty() { None } else { Some(name) },
                filesystem: fs_hint,
                is_bootable: false,
                is_active: true,
            });
        }

        Ok(partitions)
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn format_guid(bytes: &[u8]) -> String {
    if bytes.len() < 16 {
        return String::new();
    }
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_le_bytes([bytes[4], bytes[5]]),
        u16::from_le_bytes([bytes[6], bytes[7]]),
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let words: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&w| w != 0)
        .collect();
    String::from_utf16_lossy(&words).to_string()
}

fn mbr_type_name(t: u8) -> &'static str {
    match t {
        0x01 => "FAT12",
        0x04 => "FAT16 <32M",
        0x05 => "Extended CHS",
        0x06 => "FAT16",
        0x07 => "NTFS / exFAT / HPFS",
        0x0B => "FAT32 CHS",
        0x0C => "FAT32 LBA",
        0x0E => "FAT16 LBA",
        0x0F => "Extended LBA",
        0x11 => "Hidden FAT12",
        0x14 => "Hidden FAT16",
        0x16 => "Hidden FAT16",
        0x17 => "Hidden NTFS",
        0x1B => "Hidden FAT32",
        0x1C => "Hidden FAT32 LBA",
        0x27 => "Windows Recovery",
        0x42 => "Dynamic Disk",
        0x82 => "Linux Swap",
        0x83 => "Linux",
        0x85 => "Linux Extended",
        0x8E => "Linux LVM",
        0xA5 => "FreeBSD",
        0xA6 => "OpenBSD",
        0xA8 => "macOS HFS",
        0xAB => "macOS Boot",
        0xAF => "macOS HFS+",
        0xBE => "Solaris Boot",
        0xBF => "Solaris",
        0xEB => "BeOS",
        0xEE => "GPT Protective MBR",
        0xEF => "EFI System",
        0xFB => "VMware VMFS",
        0xFD => "Linux RAID",
        _ => "Unknown",
    }
}

fn guess_mbr_filesystem(t: u8) -> Option<String> {
    match t {
        0x01 => Some("FAT12".into()),
        0x04 | 0x06 | 0x0E | 0x14 | 0x16 | 0x1E => Some("FAT16".into()),
        0x0B | 0x0C | 0x1B | 0x1C => Some("FAT32".into()),
        0x07 => Some("NTFS".into()),
        0x17 => Some("NTFS".into()),
        0x83 => Some("ext4".into()),
        0xAF => Some("HFS+".into()),
        0xEF => Some("FAT32".into()),
        _ => None,
    }
}

fn gpt_type_filesystem(guid: &str) -> Option<String> {
    match guid.to_uppercase().as_str() {
        "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7" => Some("Basic Data (NTFS/FAT)".into()),
        "0FC63DAF-8483-4772-8E79-3D69D8477DE4" => Some("Linux filesystem".into()),
        "48465300-0000-11AA-AA11-00306543ECAC" => Some("HFS+".into()),
        "7C3457EF-0000-11AA-AA11-00306543ECAC" => Some("APFS".into()),
        "C12A7328-F81F-11D2-BA4B-00A0C93EC93B" => Some("EFI System".into()),
        "E3C9E316-0B5C-4DB8-817D-F92DF00215AE" => Some("Microsoft Reserved".into()),
        "DE94BBA4-06D1-4D40-A16A-BFD50179D6AC" => Some("Windows Recovery Environment".into()),
        "53746F72-6167-11AA-AA11-00306543ECAC" => Some("APFS (Storage)".into()),
        "426F6F74-0000-11AA-AA11-00306543ECAC" => Some("APFS Recovery".into()),
        "6523F8AE-3EB1-4E2A-A05A-18B695AE656F" => Some("APFS Preboot".into()),
        _ => None,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use recoverx_storage::disk_image::DiskImageSource;

    fn make_mbr_image() -> Vec<u8> {
        let mut disk = vec![0u8; 2 * 1024 * 1024]; // 2 MiB
        disk[510] = 0x55;
        disk[511] = 0xAA;
        // Entry 0: bootable FAT32 LBA starting at LBA 2, 100 sectors
        let e = &mut disk[446..462];
        e[0] = 0x80; // bootable
        e[4] = 0x0C; // FAT32 LBA
        e[8..12].copy_from_slice(&2u32.to_le_bytes());
        e[12..16].copy_from_slice(&100u32.to_le_bytes());
        disk
    }

    #[test]
    fn detects_mbr_partition() {
        let img = make_mbr_image();
        let mut src = DiskImageSource::from_bytes(img, "mbr.img".into());
        let parts = PartitionDetector::detect(&mut src).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].type_id, Some(0x0C));
        assert!(parts[0].is_bootable);
        assert_eq!(parts[0].start_lba, 2);
    }

    #[test]
    fn empty_disk_no_partitions() {
        let img = vec![0u8; 4096];
        let mut src = DiskImageSource::from_bytes(img, "empty.img".into());
        let parts = PartitionDetector::detect(&mut src).unwrap();
        assert!(parts.is_empty());
    }

    #[test]
    fn no_mbr_signature_no_partitions() {
        let mut img = vec![0u8; 4096];
        // No 0x55 0xAA signature
        img[510] = 0x00;
        img[511] = 0x00;
        let mut src = DiskImageSource::from_bytes(img, "noboot.img".into());
        let parts = PartitionDetector::detect(&mut src).unwrap();
        assert!(parts.is_empty());
    }
}
