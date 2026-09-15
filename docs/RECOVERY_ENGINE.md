# RecoverX Recovery Engine

## Architecture

```
React UI
    │  Tauri IPC (invoke / events)
    ▼
src-tauri (Tauri 2 commands)
    │
    ▼
recoverx-orchestrator  ← drives the pipeline
    │
    ├── recoverx-engines
    │       ├── PartitionDetector   (MBR / GPT)
    │       ├── FilesystemDetector  (magic bytes)
    │       ├── Fat32Analyzer       (0xE5 deleted entries)
    │       ├── NtfsAnalyzer        (MFT orphan records)
    │       ├── Ext4Analyzer        (deleted inodes)
    │       ├── ApfsAnalyzer        (stub → file carving)
    │       ├── FileCarver          (header/footer signatures)
    │       ├── TrashScanner        (macOS/Linux Trash dirs)
    │       └── RecoveryManager     (safe file output)
    │
    ├── recoverx-storage
    │       ├── PhysicalDeviceSource  (block devices)
    │       ├── DiskImageSource       (RAW/DD/IMG files)
    │       └── PartitionSource       (byte-range window)
    │
    ├── recoverx-devices
    │       ├── MacOSDeviceProvider   (diskutil CLI)
    │       ├── LinuxDeviceProvider   (/sys/block)
    │       └── WindowsDeviceProvider (PhysicalDriveN)
    │
    └── recoverx-core
            ├── EventBus            (tokio broadcast)
            ├── SourceWriteGuard    (write protection)
            ├── SourceIdentity      (resume verification)
            └── Error / Types
```

## Pipeline Stages

The orchestrator runs these stages in order for every scan:

| # | Stage | What it does |
|---|-------|--------------|
| 1 | Source Validation | Verifies the path exists, updates size if 0 |
| 2 | Partition Detection | Parses MBR or GPT from sector 0/1 |
| 3 | Filesystem Detection | Reads magic bytes at each partition offset |
| 4 | Filesystem Metadata | FAT32/NTFS/ext4 live tree walk (Quick + Deep) |
| 5 | Deleted File Analysis | FAT32 0xE5, NTFS orphan MFT, ext4 dtime>0 inode; Trash scan |
| 6 | File Carving | Header/footer scan across all raw sectors (Deep + Carving modes) |
| 7 | Result Normalization | Dedup by source offset, keep highest confidence |
| 8 | Duplicate Detection | Name+size heuristic, reduce confidence on likely dupes |
| 9 | Confidence Scoring | Boost for original path, known extension; reduce for fragmented |
| 10 | Index Results | Write all RecoveredFile records to SQLite |

## Events

Every stage emits `RecoverXEvent` items on the `EventBus` (tokio broadcast channel). The Tauri `setup()` function spawns a task that forwards every event to the Tauri window event system as `"recoverx-event"` JSON payload. The React frontend subscribes via `@tauri-apps/api/event listen()`.

```
RecoveryOrchestrator
    └── EventBus.publish(event)
            └── tokio::spawn → app_handle.emit("recoverx-event", json)
                    └── React: listen("recoverx-event", callback)
```

## Source Protection

`SourceWriteGuard` is instantiated in `RecoveryManager::new(source_path)`. It:
1. Registers the source path as protected.
2. `assert_safe_destination()` is called before any file is written.
3. If destination is on the source device, the entire recovery operation is rejected.
4. Filenames are sanitised via `sanitise_filename()` before writing.

The `StorageSource` trait itself has no write method — writes are impossible at the type level.

## Resumability

Each `ScanSession` stores `last_offset` in SQLite. On resume:
1. `SourceIdentity::is_same_source()` compares serial number (if available) or path+size.
2. If the source changed, the resume is rejected with `SourceIdentityMismatch`.
3. The pipeline currently restarts from the beginning (per-stage checkpointing is future work).
