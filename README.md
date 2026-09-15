# RecoverX

A focused, real-world **data recovery tool** for macOS, Windows, and Linux.

RecoverX recovers deleted and lost files from internal drives, external drives, USB devices, and SD cards — whenever the data is still physically recoverable. It never modifies the source device.

---

## Features

- **Real device detection** — internal SSDs, HDDs, NVMe, external drives, USB, SD cards
- **Partition detection** — MBR and GPT partition tables
- **Filesystem analysis** — FAT32, exFAT, NTFS, ext2/3/4, APFS (detection), HFS+ (detection)
- **Deleted file recovery** — directory entry scanning (FAT32 0xE5), MFT orphan records (NTFS), inode scanning (ext4)
- **File carving** — signature-based raw sector scanning for 25+ file types
- **Trash / Recycle Bin recovery** — macOS .Trash/.Trashes, Linux ~/.local/share/Trash
- **Safe recovery** — SourceWriteGuard enforces read-only access; atomic writes with SHA-256 verification
- **Real-time progress** — Tauri event bus delivers live scan progress to the UI
- **Three scan modes** — Quick Scan, Deep Scan, File Carving

---

## Supported Platforms

| Platform | Status |
|----------|--------|
| macOS (Apple Silicon + Intel) | ✅ Primary |
| Linux x64 | ✅ Supported |
| Windows x64 | ✅ Architecture ready |

---

## Supported Filesystems

| Filesystem | Detection | Deleted Entry Recovery | Notes |
|------------|-----------|------------------------|-------|
| FAT32 | ✅ | ✅ | 0xE5 directory entry scan |
| exFAT | ✅ | ✅ (via FAT32 path) | |
| NTFS | ✅ | ✅ | MFT orphan record scan |
| ext2/3/4 | ✅ | ✅ | Inode dtime scan |
| APFS | ✅ (detection) | 🔄 (carving fallback) | Proprietary format |
| HFS+ | ✅ (detection) | 🔄 (carving fallback) | |
| ISO 9660 | ✅ (detection) | — | Read-only media |

---

## Scan Modes

### Quick Scan
Scans filesystem metadata and deleted directory entries. Completes in minutes.
- Partition detection
- Filesystem identification
- Deleted entry recovery (FAT32, NTFS, ext4)
- Trash/Recycle Bin scan

### Deep Scan
Full analysis including file carving. Most thorough mode.
- Everything in Quick Scan
- Raw sector file carving (25+ signatures)
- Duplicate detection
- SHA-256 hash verification

### File Carving
Signature-based raw sector scan. Use when the filesystem is damaged or formatted.
- No filesystem metadata required
- Header/footer detection for 25+ file types
- Confidence scoring per recovered file

---

## Supported File Types (Carving)

Images: JPG, PNG, GIF, BMP, TIFF, WEBP  
Documents: PDF, DOC/DOCX, XLS/XLSX, PPT/PPTX, TXT  
Archives: ZIP, RAR, 7Z, GZ  
Audio: MP3 (ID3 + sync), WAV, FLAC, OGG  
Video: MP4, MOV, AVI, MKV  
Databases: SQLite  
Other: XML, HTML  

---

## Installation

### Prerequisites

- Rust 1.78+ (`rustup update stable`)
- Node.js 20+ and npm
- Tauri CLI v2 (`cargo install tauri-cli`)

### Build

```bash
git clone https://github.com/recoverx/recoverx
cd recoverx
npm install
cargo tauri build
```

### Development

```bash
npm install
cargo tauri dev
```

---

## Running

### GUI

```bash
# macOS/Linux — elevated privileges required for raw device access
sudo cargo tauri dev
```

### CLI

```bash
# List devices
cargo run -p recoverx-cli -- devices

# Start a deep scan
cargo run -p recoverx-cli -- scan --source /dev/disk2 --mode deep

# List sessions
cargo run -p recoverx-cli -- sessions list
```

---

## Safety

- All source devices are opened **read-only** (`O_RDONLY`).
- `SourceWriteGuard` blocks any write attempt to a registered source path.
- Recovery destinations are validated — writing to the source device is blocked.
- Recovered filenames are sanitised to prevent path traversal.
- Recovered files are never executed.
- SHA-256 is computed for every recovered file.

---

## SSD and TRIM Limitations

Recovery success depends on storage technology:

| Technology | Recoverability |
|------------|---------------|
| HDD | Generally recoverable until overwritten |
| USB flash drive | Often recoverable; depends on controller |
| SD card | Often recoverable; depends on controller |
| SSD (no TRIM) | Possibly recoverable |
| SSD (TRIM enabled) | Limited — deleted blocks may be erased immediately |
| NVMe SSD | Limited — same TRIM considerations |
| FileVault / BitLocker | Not recoverable without key |

RecoverX never claims 100% recovery. Each file is assigned a realistic confidence score.

---

## Testing

```bash
# Run all tests (no real disk access required)
cargo test

# Run integration tests only
cargo test -p recoverx-engines

# Run frontend type check
npm run build
```

See [docs/TESTING.md](docs/TESTING.md) for details on the synthetic test image suite.

---

## Documentation

- [docs/RECOVERY_ENGINE.md](docs/RECOVERY_ENGINE.md) — Pipeline architecture
- [docs/FILESYSTEM_SUPPORT.md](docs/FILESYSTEM_SUPPORT.md) — Per-filesystem details
- [docs/PLATFORM_ACCESS.md](docs/PLATFORM_ACCESS.md) — OS-specific device access
- [docs/SECURITY.md](docs/SECURITY.md) — Security model and guarantees
- [docs/LIMITATIONS.md](docs/LIMITATIONS.md) — Honest limitations
- [docs/TESTING.md](docs/TESTING.md) — Test strategy and synthetic images

---

## License

MIT OR Apache-2.0
