# Testing

## Running Tests

```bash
# All workspace tests (no real disk required)
cargo test

# Engine unit + integration tests only
cargo test -p recoverx-engines

# Frontend type-check and build
npm run build
```

## Test Coverage

### Unit Tests (per crate)

| Crate | Tests | What's tested |
|-------|-------|---------------|
| `recoverx-core` | 9 | SourceWriteGuard, sanitise_filename, path traversal, SourceIdentity |
| `recoverx-storage` | 8 | DiskImageSource read/seek, PartitionSource bounds, SourceIdentity |
| `recoverx-orchestrator` | 16 | SessionDatabase CRUD, ScanSession state machine, orchestrator lifecycle, cancel persistence |
| `recoverx-engines` (unit) | 14 | FilesystemDetector (NTFS/FAT32/ext4/unknown), PartitionDetector (MBR), FileCarver (JPEG/PNG), RecoveryManager, TrashScanner |

### Integration Tests (`crates/recoverx-engines/tests/`)

| Test | What it verifies |
|------|-----------------|
| `fat32_filesystem_detected` | FAT32 BPB magic correctly identified |
| `fat32_deleted_file_found` | 0xE5 entry found with correct metadata |
| `source_not_modified_after_fat32_scan` | SHA-256 of image unchanged after scan |
| `file_carving_finds_jpeg` | JPEG at offset 65536 found with correct offset |
| `source_not_modified_after_carving` | SHA-256 of image unchanged after carving |
| `mbr_partition_table_detected` | FAT32-LBA partition entry parsed correctly |
| `recovery_manager_writes_file_correctly` | Content matches, SHA-256 matches, size correct |
| `recovery_blocked_when_destination_is_source` | SourceWriteGuard blocks same-source write |
| `collision_handling_does_not_overwrite` | Existing file kept; new file gets `_1` suffix |
| `trash_scanner_finds_files` | 2 files found, .trashinfo skipped, confidence=90 |
| `no_fake_files_when_disk_is_empty` | Zero-byte disk yields zero carved results |

## Synthetic Disk Images

All tests use in-memory synthetic disk images built by helpers in `integration_test.rs`:

- `build_fat32_image_with_deleted_file(content)` — 512 KiB FAT32 with one `0xE5` deleted entry pointing to a data cluster
- `build_image_with_jpeg(jpeg_data)` — 1 MiB raw image with a JPEG embedded at offset 65536
- `minimal_jpeg()` — SOI + APP0 + EOI bytes

No real disk is read or written during tests.

## Testing on Real Devices (Manual)

To test on a real device safely:

1. Create a test disk image with `dd`:
   ```bash
   dd if=/dev/zero of=/tmp/test_fat32.img bs=1M count=64
   ```

2. Format it as FAT32 (macOS):
   ```bash
   hdiutil attach /tmp/test_fat32.img
   diskutil eraseDisk FAT32 TESTDISK /dev/disk3  # use correct disk number
   ```

3. Copy test files, then delete them and empty the Trash.

4. Detach the image:
   ```bash
   hdiutil detach /dev/disk3
   ```

5. Run RecoverX against `/tmp/test_fat32.img` (no raw device privileges required for image files).

6. Verify SHA-256 of the image matches before and after:
   ```bash
   shasum -a 256 /tmp/test_fat32.img
   ```

## Continuous Integration

Tests are designed to run in CI without root access or real hardware. The full `cargo test` suite completes in under 5 seconds.
