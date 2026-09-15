//! Filesystem type detection from raw superblock bytes.

use recoverx_core::error::Result;
use recoverx_storage::source::StorageSource;

use crate::models::{DetectedFilesystem, FilesystemType};

pub struct FilesystemDetector;

impl FilesystemDetector {
    /// Detect filesystem at `offset` within `source`.
    pub fn detect(source: &mut dyn StorageSource, offset: u64) -> Result<DetectedFilesystem> {
        let sector_size = source.sector_size();
        let available = source.size().saturating_sub(offset);
        if available == 0 {
            return Ok(Self::unknown(offset, sector_size));
        }

        // Read first 4 KiB for most signatures
        let read_size = (4096usize).min(available as usize);
        let mut buf = Vec::new();
        source.read(offset, read_size, &mut buf)?;

        // Read at +1024 for ext superblock and HFS+
        let mut ext_buf = Vec::new();
        if available > 1024 + 512 {
            let _ = source.read(offset + 1024, 512.min((available - 1024) as usize), &mut ext_buf);
        }

        // NTFS: OEM ID "NTFS    " at bytes 3–10
        if buf.len() >= 11 && &buf[3..7] == b"NTFS" {
            return Ok(Self::parse_ntfs(&buf, offset, sector_size));
        }

        // exFAT: OEM ID "EXFAT   " at bytes 3–10
        if buf.len() >= 11 && &buf[3..11] == b"EXFAT   " {
            return Ok(Self::parse_exfat(&buf, offset, sector_size));
        }

        // FAT32: "FAT32   " at offset 82 + boot signature
        if buf.len() >= 512 && buf[510] == 0x55 && buf[511] == 0xAA {
            if buf.len() >= 90 && &buf[82..90] == b"FAT32   " {
                return Ok(Self::parse_fat32(&buf, offset, sector_size));
            }
            // FAT12/16: "FAT" at offset 54
            if buf.len() >= 57 && &buf[54..57] == b"FAT" {
                return Ok(Self::parse_fat16(&buf, offset, sector_size));
            }
        }

        // ext2/3/4: magic 0xEF53 at offset 56 within superblock (which starts at 1024)
        if ext_buf.len() >= 58 {
            let magic = u16::from_le_bytes([ext_buf[56], ext_buf[57]]);
            if magic == 0xEF53 {
                return Ok(Self::parse_ext(&ext_buf, offset, sector_size));
            }
        }

        // APFS: "NXSB" container superblock magic at offset 32
        if buf.len() >= 36 && &buf[32..36] == b"NXSB" {
            return Ok(DetectedFilesystem {
                fs_type: FilesystemType::Apfs,
                label: Some("APFS Container".into()),
                uuid: None,
                total_size: source.size().saturating_sub(offset),
                free_size: None,
                cluster_size: 4096,
                sector_size,
                offset_in_source: offset,
            });
        }

        // HFS+: magic 0x482B or 0x4858 at start of volume header (offset 1024)
        if ext_buf.len() >= 2 {
            let hfs_magic = u16::from_be_bytes([ext_buf[0], ext_buf[1]]);
            if hfs_magic == 0x482B || hfs_magic == 0x4858 {
                return Ok(DetectedFilesystem {
                    fs_type: FilesystemType::HfsPlus,
                    label: None,
                    uuid: None,
                    total_size: source.size().saturating_sub(offset),
                    free_size: None,
                    cluster_size: 4096,
                    sector_size,
                    offset_in_source: offset,
                });
            }
        }

        // ISO 9660: primary volume descriptor at sector 16 * 2048
        let iso_offset = offset + 16 * 2048;
        if source.size() > iso_offset + 8 {
            let mut iso_buf = Vec::new();
            let _ = source.read(iso_offset, 8, &mut iso_buf);
            if iso_buf.len() >= 6 && &iso_buf[1..6] == b"CD001" {
                return Ok(DetectedFilesystem {
                    fs_type: FilesystemType::Iso9660,
                    label: None,
                    uuid: None,
                    total_size: source.size().saturating_sub(offset),
                    free_size: None,
                    cluster_size: 2048,
                    sector_size,
                    offset_in_source: offset,
                });
            }
        }

        Ok(Self::unknown(offset, sector_size))
    }

    fn unknown(offset: u64, sector_size: u32) -> DetectedFilesystem {
        DetectedFilesystem {
            fs_type: FilesystemType::Unknown,
            label: None,
            uuid: None,
            total_size: 0,
            free_size: None,
            cluster_size: sector_size,
            sector_size,
            offset_in_source: offset,
        }
    }

    fn parse_ntfs(buf: &[u8], offset: u64, sector_size: u32) -> DetectedFilesystem {
        let bps = if buf.len() >= 13 {
            u16::from_le_bytes([buf[11], buf[12]]) as u64
        } else {
            512
        };
        let spc = if buf.len() >= 14 { buf[13] as u64 } else { 8 };
        let cluster_size = (bps * spc) as u32;
        let total_sectors = if buf.len() >= 48 {
            u64::from_le_bytes(buf[40..48].try_into().unwrap_or([0; 8]))
        } else {
            0
        };
        DetectedFilesystem {
            fs_type: FilesystemType::Ntfs,
            label: None,
            uuid: None,
            total_size: total_sectors * bps,
            free_size: None,
            cluster_size,
            sector_size,
            offset_in_source: offset,
        }
    }

    fn parse_fat32(buf: &[u8], offset: u64, sector_size: u32) -> DetectedFilesystem {
        let bps = u16::from_le_bytes([buf[11], buf[12]]) as u64;
        let spc = buf[13] as u64;
        let cluster_size = ((bps * spc) as u32).max(512);
        let total_sectors = if u16::from_le_bytes([buf[19], buf[20]]) == 0 {
            u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]) as u64
        } else {
            u16::from_le_bytes([buf[19], buf[20]]) as u64
        };
        // Volume label at offset 71, 11 bytes
        let label = if buf.len() >= 82 {
            let raw = &buf[71..82];
            let s = String::from_utf8_lossy(raw).trim().to_string();
            if s.is_empty() || s == "NO NAME" {
                None
            } else {
                Some(s)
            }
        } else {
            None
        };
        DetectedFilesystem {
            fs_type: FilesystemType::Fat32,
            label,
            uuid: None,
            total_size: total_sectors * bps,
            free_size: None,
            cluster_size,
            sector_size,
            offset_in_source: offset,
        }
    }

    fn parse_fat16(buf: &[u8], offset: u64, sector_size: u32) -> DetectedFilesystem {
        let bps = u16::from_le_bytes([buf[11], buf[12]]) as u64;
        let spc = buf[13] as u64;
        let cluster_size = ((bps * spc) as u32).max(512);
        let total_sectors = if u16::from_le_bytes([buf[19], buf[20]]) == 0 {
            u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]) as u64
        } else {
            u16::from_le_bytes([buf[19], buf[20]]) as u64
        };
        DetectedFilesystem {
            fs_type: FilesystemType::Fat32, // FAT16 uses same analysis path
            label: None,
            uuid: None,
            total_size: total_sectors * bps,
            free_size: None,
            cluster_size,
            sector_size,
            offset_in_source: offset,
        }
    }

    fn parse_exfat(buf: &[u8], offset: u64, sector_size: u32) -> DetectedFilesystem {
        let bps_exp = if buf.len() > 108 { buf[108] as u32 } else { 9 };
        let spc_exp = if buf.len() > 109 { buf[109] as u32 } else { 3 };
        let cluster_size = (1u32 << bps_exp) * (1u32 << spc_exp);
        let total_clusters = if buf.len() >= 116 {
            u32::from_le_bytes(buf[112..116].try_into().unwrap_or([0; 4]))
        } else {
            0
        };
        DetectedFilesystem {
            fs_type: FilesystemType::ExFat,
            label: None,
            uuid: None,
            total_size: total_clusters as u64 * cluster_size as u64,
            free_size: None,
            cluster_size,
            sector_size,
            offset_in_source: offset,
        }
    }

    fn parse_ext(ext_buf: &[u8], offset: u64, sector_size: u32) -> DetectedFilesystem {
        let block_count = if ext_buf.len() >= 8 {
            u32::from_le_bytes(ext_buf[4..8].try_into().unwrap_or([0; 4])) as u64
        } else {
            0
        };
        let block_size_shift = if ext_buf.len() >= 28 {
            u32::from_le_bytes(ext_buf[24..28].try_into().unwrap_or([0; 4]))
        } else {
            1
        };
        let block_size = 1024u32 << block_size_shift;

        let feature_incompat = if ext_buf.len() >= 100 {
            u32::from_le_bytes(ext_buf[96..100].try_into().unwrap_or([0; 4]))
        } else {
            0
        };
        let fs_type = if feature_incompat & 0x40 != 0 {
            FilesystemType::Ext4
        } else if feature_incompat & 0x01 != 0 {
            FilesystemType::Ext3
        } else {
            FilesystemType::Ext2
        };

        // Volume label at offset 120, 16 bytes
        let label = if ext_buf.len() >= 136 {
            let s = String::from_utf8_lossy(&ext_buf[120..136])
                .trim_end_matches('\0')
                .trim()
                .to_string();
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        };

        // UUID at offset 104
        let uuid = if ext_buf.len() >= 120 {
            let b = &ext_buf[104..120];
            Some(format!(
                "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
            ))
        } else {
            None
        };

        DetectedFilesystem {
            fs_type,
            label,
            uuid,
            total_size: block_count * block_size as u64,
            free_size: None,
            cluster_size: block_size,
            sector_size,
            offset_in_source: offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recoverx_storage::disk_image::DiskImageSource;

    fn make_ntfs_image() -> Vec<u8> {
        let mut buf = vec![0u8; 4096];
        // OEM ID "NTFS    "
        buf[3..11].copy_from_slice(b"NTFS    ");
        buf[11] = 0x00;
        buf[12] = 0x02; // bytes per sector = 512
        buf[13] = 8; // sectors per cluster
        // total sectors
        let ts: u64 = 2_000_000;
        buf[40..48].copy_from_slice(&ts.to_le_bytes());
        buf
    }

    fn make_fat32_image() -> Vec<u8> {
        let mut buf = vec![0u8; 512];
        buf[82..90].copy_from_slice(b"FAT32   ");
        buf[11] = 0x00;
        buf[12] = 0x02; // 512 bytes per sector
        buf[13] = 8; // 8 sectors per cluster
        buf[510] = 0x55;
        buf[511] = 0xAA;
        buf
    }

    fn make_ext4_image() -> Vec<u8> {
        let mut buf = vec![0u8; 2048];
        // ext magic at offset 1024+56 = 1080
        buf[1080] = 0x53;
        buf[1081] = 0xEF;
        // feature_incompat with extents bit (0x40) at offset 1024+96 = 1120
        let fi: u32 = 0x40;
        buf[1120..1124].copy_from_slice(&fi.to_le_bytes());
        // block size shift = 2 => 4096
        let shift: u32 = 2;
        buf[1048..1052].copy_from_slice(&shift.to_le_bytes());
        buf
    }

    #[test]
    fn detects_ntfs() {
        let mut src = DiskImageSource::from_bytes(make_ntfs_image(), "ntfs.img".into());
        let fs = FilesystemDetector::detect(&mut src, 0).unwrap();
        assert_eq!(fs.fs_type, FilesystemType::Ntfs);
    }

    #[test]
    fn detects_fat32() {
        let mut src = DiskImageSource::from_bytes(make_fat32_image(), "fat32.img".into());
        let fs = FilesystemDetector::detect(&mut src, 0).unwrap();
        assert_eq!(fs.fs_type, FilesystemType::Fat32);
    }

    #[test]
    fn detects_ext4() {
        let mut src = DiskImageSource::from_bytes(make_ext4_image(), "ext4.img".into());
        let fs = FilesystemDetector::detect(&mut src, 0).unwrap();
        assert_eq!(fs.fs_type, FilesystemType::Ext4);
    }

    #[test]
    fn unknown_returns_unknown() {
        let data = vec![0u8; 512];
        let mut src = DiskImageSource::from_bytes(data, "rand.img".into());
        let fs = FilesystemDetector::detect(&mut src, 0).unwrap();
        assert_eq!(fs.fs_type, FilesystemType::Unknown);
    }
}
