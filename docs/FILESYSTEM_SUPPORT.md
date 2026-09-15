# Filesystem Support

## FAT32 / exFAT

**Detection:** OEM ID `"FAT32   "` at BPB offset 82; boot signature `0x55 0xAA`.

**Deleted file recovery:** Scans all directory clusters for entries where the first byte is `0xE5` (deleted marker). Reconstructs:
- Filename (first character replaced with `_` since it was overwritten with `0xE5`)
- File size from the directory entry
- Start cluster from high/low words

**Limitations:**
- Long File Name (LFN) entries are skipped; 8.3 name only.
- If the FAT chain has been partially reused, the cluster extent may be incorrect.
- Fragmented files are returned with a lower confidence score.

## NTFS

**Detection:** OEM ID `"NTFS    "` at BPB offset 3.

**Deleted file recovery:** Scans the Master File Table (MFT) for `FILE` records where the flags field has bit 0 clear (not in use). Parses:
- `$FILE_NAME` attribute for the original filename
- `$DATA` attribute for file size and data location

**Limitations:**
- Non-resident data runs require further parsing for fragmented files (returns `source_offset = record_offset` as a hint).
- MFT mirror (`$MFTMirr`) is not consulted.
- `$LogFile` / `$UsnJrnl` analysis not yet implemented.

## ext2 / ext3 / ext4

**Detection:** Magic `0xEF53` at superblock offset 56 (superblock at volume offset + 1024).

**Deleted file recovery:** Iterates block groups via the block group descriptor table. For each group, reads the inode table and finds inodes where:
- Mode indicates regular file (`0x8000`)
- `dtime` (deletion time) is non-zero

**Limitations:**
- Original filename cannot be reconstructed from the inode alone (directory scan required — not yet implemented).
- Recovered files are named `deleted_inode_N.bin` with low confidence.
- ext4 extents for fragmented files are not fully parsed.

## APFS

**Detection:** `"NXSB"` container superblock magic at offset 32.

**Recovery:** APFS uses a proprietary B-tree structure that is not publicly documented. RecoverX detects APFS volumes and uses **file carving** as a fallback to recover files by signature. No APFS-native deleted file recovery.

## HFS+

**Detection:** Magic `0x482B` or `0x4858` at volume header offset 1024.

**Recovery:** Falls back to file carving. HFS+ Catalog B-tree parsing is not implemented.

## Partition Tables

### MBR
- Reads 4 primary partition entries from sector 0 (offset 446).
- Identifies partition types (FAT32, NTFS, Linux, etc.) by type byte.
- Extended partitions are not followed.

### GPT
- Reads GPT header from LBA 1.
- Parses up to 256 partition entries.
- Maps type GUIDs to human-readable names (APFS, EFI System, Linux, etc.).
