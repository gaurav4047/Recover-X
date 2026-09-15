# Known Limitations

## Storage Technology

| Situation | Recoverability |
|-----------|---------------|
| SSD with TRIM (most modern SSDs) | Limited to none — deleted blocks are erased |
| SSD without TRIM | Possibly recoverable |
| HDD | Generally recoverable until space is reused |
| USB flash drive | Often recoverable; controller-dependent |
| SD / microSD card | Often recoverable; controller-dependent |
| Encrypted volume (FileVault, BitLocker) | Not recoverable without key |
| Overwritten data | Not recoverable |
| Reformatted and rewritten partition | Not recoverable |

## Filesystem Coverage

- **APFS:** RecoverX can detect APFS volumes but has no native APFS parser. File carving is the only recovery method on APFS. This is a significant limitation on modern Macs where the system volume and user data are always APFS.
- **HFS+:** Detection only; no B-tree parsing. File carving only.
- **NTFS fragmented files:** Data runs are not fully parsed for non-resident attributes, so fragmented NTFS files may have incorrect sizes or offsets.
- **ext4 filenames:** Cannot reconstruct original filenames from inodes alone (requires directory scan, not yet implemented). Files are named `deleted_inode_N.bin`.
- **FAT32 LFN:** Long File Name entries are skipped; only 8.3 names are recovered.
- **exFAT:** Uses the FAT32 code path; some exFAT-specific structures are not parsed.

## Platform Coverage

- **macOS model/serial:** IOKit bindings are not implemented; `model` and `serial` are always `None` in device listings.
- **Windows device size:** `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` is not called; size is reported as 0 for Windows physical devices.
- **Windows model/serial:** WMI queries are not implemented.
- **Linux filesystem type:** `blkid` integration is not implemented; filesystem type from device enumeration is `None`.

## File Carving

- Carving cannot reconstruct the original filename or directory path.
- Carved files are named `carved_NNNNNN.ext`.
- Fragmented files (where the data is split across non-contiguous sectors) will not be correctly reassembled by the carver.
- Very large files (e.g., 50 GB video files) may have their size capped at `max_size` in the signature database.
- False positives are possible — not every match of a file header is a complete, valid file.

## Performance

- The carver reads the entire source sequentially. For a 1 TB drive this takes several hours.
- No per-stage resume checkpointing — if a scan is cancelled mid-way, it restarts from the beginning.
- The SQLite `recovered_files` table is not paginated in the UI; very large result sets (>100,000 files) may be slow to render.

## What RecoverX Does Not Do

- Repair corrupted filesystems.
- Format or partition disks.
- Mount volumes.
- Recover encrypted data without the key.
- Support E01/AFF forensic image formats.
- Provide chain-of-custody or case management.
