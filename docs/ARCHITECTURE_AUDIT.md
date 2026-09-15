# RecoverX Architecture Audit

**Audit Date:** 2026-09-11  
**Auditor:** Automated full-source audit (kiro/audit)  
**Phase:** 1 — Foundation  
**Version:** 0.1.0

---

## Table of Contents

1. [Current Architecture Map](#1-current-architecture-map)
2. [What Is Actually Implemented vs Stub](#2-what-is-actually-implemented-vs-stub)
3. [Phase 2 Placeholder Locations — Complete List](#3-phase-2-placeholder-locations--complete-list)
4. [Tauri IPC Chain Status](#4-tauri-ipc-chain-status)
5. [Device Enumeration Status](#5-device-enumeration-status)
6. [StorageSource Status](#6-storagesource-status)
7. [RecoveryOrchestrator Status](#7-recoveryorchestrator-status)
8. [Filesystem Engines](#8-filesystem-engines)
9. [File Carving Engine](#9-file-carving-engine)
10. [Results Browser](#10-results-browser)
11. [Recovery Manager](#11-recovery-manager)
12. [Report Engine](#12-report-engine)
13. [SQLite Schema](#13-sqlite-schema)
14. [Security Layer Status](#14-security-layer-status)
15. [Known Bugs and Logic Gaps](#15-known-bugs-and-logic-gaps)
16. [Dependency Inventory](#16-dependency-inventory)
17. [All Missing Implementations](#17-all-missing-implementations)
18. [Recommended Implementation Order for Phase 2](#18-recommended-implementation-order-for-phase-2)

---

## 1. Current Architecture Map

```
recoverx/
├── Cargo.toml                      Workspace root (resolver = "2")
├── package.json                    Frontend deps: React 19, Tauri API v2, react-router-dom 7
│
├── crates/
│   ├── recoverx-core               Foundational types — NO I/O
│   │   ├── error.rs                RecoverXError enum (14 variants)
│   │   ├── events.rs               RecoverXEvent enum + EventBus (broadcast channel)
│   │   ├── identity.rs             SourceIdentity — resume integrity check
│   │   ├── security.rs             SourceWriteGuard, sanitise_filename, path traversal guard
│   │   └── types.rs                SessionId, TaskId, ScanMode, ScanConfiguration, enums
│   │
│   ├── recoverx-storage            StorageSource abstraction — READ-ONLY
│   │   ├── source.rs               StorageSource trait + StorageSourceMetadata
│   │   ├── physical_device.rs      PhysicalDeviceSource (file I/O + macOS/Linux ioctl size)
│   │   ├── disk_image.rs           DiskImageSource (RAW/DD/IMG, with partial SHA-256)
│   │   ├── forensic_image.rs       ForensicImageSource — STUB (always errors)
│   │   └── partition.rs            PartitionSource — byte-range window over any source
│   │
│   ├── recoverx-devices            Platform-specific enumeration
│   │   ├── provider.rs             DeviceProvider trait + DeviceInfo + DeviceType
│   │   ├── macos.rs                MacOSDeviceProvider (diskutil CLI)
│   │   ├── linux.rs                LinuxDeviceProvider (/sys/block)
│   │   └── windows.rs              WindowsDeviceProvider (PhysicalDriveN probe)
│   │
│   ├── recoverx-orchestrator       Pipeline coordination + persistence
│   │   ├── orchestrator.rs         RecoveryOrchestrator — ALL 10 PIPELINE TASKS ARE STUBS
│   │   ├── session.rs              ScanSession model + state transitions
│   │   ├── task.rs                 PipelineTask + TaskType + build_pipeline()
│   │   ├── database.rs             SQLite persistence (rusqlite, WAL mode)
│   │   └── worker.rs               WorkerPool — infrastructure complete, carving logic absent
│   │
│   └── recoverx-logging            tracing/tracing-subscriber wrapper
│
├── src-tauri/                      Tauri 2 backend (GUI binary)
│   ├── src/lib.rs                  run() — registers all commands, initialises state
│   ├── src/state.rs                AppState (OnceLock<RecoveryOrchestrator>)
│   └── src/commands/
│       ├── app.rs                  get_app_info — hardcoded "Phase 1"
│       ├── devices.rs              list_devices, get_device_info
│       ├── sessions.rs             create_session, list_sessions, load_session, delete_session
│       └── scan.rs                 start_scan, pause_scan, resume_scan, cancel_scan
│
├── recoverx-cli/                   Standalone CLI (shares all crates)
│   └── src/main.rs                 clap CLI: devices | sessions | scan
│
└── src/                            React frontend
    ├── lib/api.ts                  Tauri invoke() wrappers — all typed
    ├── App.tsx                     Router (9 routes)
    ├── components/Sidebar.tsx      Navigation
    └── screens/
        ├── HomeScreen.tsx          ✅ Functional landing page
        ├── DevicesScreen.tsx       ✅ Real device listing via IPC
        ├── ScanSetupScreen.tsx     ✅ Session creation via IPC
        ├── ScanningScreen.tsx      ✅ Polls session; shows pipeline progress
        ├── SessionsScreen.tsx      ✅ List/delete sessions via IPC
        ├── ResultsScreen.tsx       ❌ Phase 2 placeholder — empty state only
        ├── RecoveryScreen.tsx      ❌ Phase 2 placeholder — empty state only
        ├── ReportsScreen.tsx       ❌ Phase 2 placeholder — empty state only
        └── SettingsScreen.tsx      ⚠️  Mostly disabled/cosmetic; security notice correct
```

### Crate Dependency Graph

```
recoverx-core   (no internal deps)
      ↑
recoverx-storage ← recoverx-core
      ↑
recoverx-devices ← recoverx-core, recoverx-storage
      ↑
recoverx-logging ← recoverx-core
      ↑
recoverx-orchestrator ← recoverx-core, recoverx-storage, recoverx-logging
      ↑
src-tauri / recoverx-cli ← all of the above
```

---

## 2. What Is Actually Implemented vs Stub

### ✅ FULLY IMPLEMENTED (Phase 1)

| Component | Location | Notes |
|-----------|----------|-------|
| Error type | `recoverx-core/src/error.rs` | 14 variants, fully mapped |
| Event system | `recoverx-core/src/events.rs` | 13 event types, broadcast channel, `EventBus` |
| Source identity | `recoverx-core/src/identity.rs` | Comparison logic + 4 unit tests |
| Security guard | `recoverx-core/src/security.rs` | `SourceWriteGuard`, filename sanitiser, path traversal guard + 9 unit tests |
| Core types | `recoverx-core/src/types.rs` | `ScanMode`, `ScanConfiguration`, `SessionId`, `TaskId`, enums |
| StorageSource trait | `recoverx-storage/src/source.rs` | Read-only contract, metadata, `read_sector` helpers |
| PhysicalDeviceSource | `recoverx-storage/src/physical_device.rs` | Opens block devices; macOS `DKIOCGETBLOCKCOUNT` ioctl for size |
| DiskImageSource | `recoverx-storage/src/disk_image.rs` | RAW/DD/IMG, partial SHA-256, in-memory backend for tests; 5 unit tests |
| PartitionSource | `recoverx-storage/src/partition.rs` | Windowed view; bounds enforcement; 3 unit tests |
| macOS device enumeration | `recoverx-devices/src/macos.rs` | `diskutil list` + `diskutil info` parsing |
| Linux device enumeration | `recoverx-devices/src/linux.rs` | `/sys/block` + model/serial from sysfs |
| Windows device enumeration | `recoverx-devices/src/windows.rs` | `PhysicalDriveN` probe via `CreateFileW` |
| DeviceProvider trait | `recoverx-devices/src/provider.rs` | `list_devices`, `get_device_info`, `DeviceInfo` |
| ScanSession model | `recoverx-orchestrator/src/session.rs` | Full state machine; 5 unit tests |
| SQLite persistence | `recoverx-orchestrator/src/database.rs` | WAL, migration v1, full CRUD; 5 unit tests |
| PipelineTask model | `recoverx-orchestrator/src/task.rs` | 10 task types, `build_pipeline()`; 3 unit tests |
| WorkerPool infrastructure | `recoverx-orchestrator/src/worker.rs` | Bounded crossbeam channels, atomic counters, cancellation flag |
| RecoveryOrchestrator API | `recoverx-orchestrator/src/orchestrator.rs` | Session lifecycle, event emission, pipeline dispatch; 3 integration tests |
| Logging | `recoverx-logging/src/lib.rs` | `tracing-subscriber`, JSON/pretty, file sink hooks, idempotent init |
| Tauri command registration | `src-tauri/src/lib.rs` | 11 commands registered |
| AppState | `src-tauri/src/state.rs` | `OnceLock` orchestrator, data dir init |
| IPC: devices | `src-tauri/src/commands/devices.rs` | `list_devices`, `get_device_info` |
| IPC: sessions | `src-tauri/src/commands/sessions.rs` | `create_session`, `list_sessions`, `load_session`, `delete_session` |
| IPC: scan | `src-tauri/src/commands/scan.rs` | `start_scan`, `pause_scan`, `resume_scan`, `cancel_scan` (bug — see §15) |
| Frontend API bindings | `src/lib/api.ts` | All IPC calls typed; `formatBytes`, `sessionStatusColor` helpers |
| Frontend routing | `src/App.tsx` | 9 routes |
| Devices screen | `src/screens/DevicesScreen.tsx` | Real IPC call, privilege notice, Quick/Deep scan buttons |
| Scan setup screen | `src/screens/ScanSetupScreen.tsx` | Source path, mode selection, engine flags, session creation |
| Scanning screen | `src/screens/ScanningScreen.tsx` | Progress bar, pipeline stage animation, polling, pause/cancel |
| Sessions screen | `src/screens/SessionsScreen.tsx` | List/delete sessions |
| CLI | `recoverx-cli/src/main.rs` | `devices`, `sessions list/show/delete`, `scan` subcommands |

### ❌ STUB / NOT IMPLEMENTED (Phase 2 required)

| Component | Status | Location |
|-----------|--------|----------|
| Source validation (engine) | Stub — logs only | `orchestrator.rs:294` |
| Partition detection (MBR/GPT) | Stub — logs only | `orchestrator.rs:298` |
| Filesystem detection | Stub — logs only | `orchestrator.rs:302` |
| Filesystem metadata analysis | Stub — logs only | `orchestrator.rs:306` |
| Deleted file analysis | Stub — logs only | `orchestrator.rs:310` |
| File carving engine | Stub — logs only | `orchestrator.rs:314` |
| Result normalization | Stub — logs only | `orchestrator.rs:318` |
| Duplicate detection | Stub — logs only | `orchestrator.rs:322` |
| Confidence scoring | Stub — logs only | `orchestrator.rs:326` |
| Index results | Stub — logs only | `orchestrator.rs:330` |
| ForensicImageSource (E01/AFF4) | Always returns error | `forensic_image.rs:28–40` |
| macOS model/serial (IOKit) | `None` | `macos.rs:189–190` |
| macOS filesystem detection | Not called | `macos.rs` |
| Linux filesystem detection (blkid) | `None` | `linux.rs:121` |
| Windows device size (IOCTL) | `0` | `windows.rs:93` |
| Windows model/serial (WMI) | `None` | `windows.rs:98–99` |
| Physical device model/serial | `None` | `physical_device.rs:63–64` |
| Sector size detection (4Kn) | Hardcoded 512 | `physical_device.rs:147` |
| WorkerPool carving logic | Empty vec | `worker.rs:133` |
| Results browser (UI) | Phase 2 placeholder | `ResultsScreen.tsx` |
| Recovery manager (UI + backend) | Phase 2 placeholder | `RecoveryScreen.tsx` |
| Report engine (UI + backend) | Phase 2 placeholder | `ReportsScreen.tsx` |
| Settings persistence | Disabled/cosmetic | `SettingsScreen.tsx` |
| cancel_scan persistence | Bug — not persisted | `scan.rs:85–91` |

---

## 3. Phase 2 Placeholder Locations — Complete List

### Rust Source Files

| File | Line | Description |
|------|------|-------------|
| `crates/recoverx-devices/src/macos.rs` | 6 | Comment: IOKit bindings deferred to Phase 2 |
| `crates/recoverx-devices/src/macos.rs` | 189 | `model: None` — Phase 2: IOKit |
| `crates/recoverx-devices/src/macos.rs` | 190 | `serial: None` — Phase 2: IOKit |
| `crates/recoverx-devices/src/linux.rs` | 121 | `filesystem: None` — Phase 2: blkid |
| `crates/recoverx-devices/src/windows.rs` | 5 | Comment: WMI queries deferred to Phase 2 |
| `crates/recoverx-devices/src/windows.rs` | 93 | `size_bytes: 0` — Phase 2: IOCTL_DISK_GET_DRIVE_GEOMETRY_EX |
| `crates/recoverx-devices/src/windows.rs` | 98 | `model: None` — Phase 2: WMI |
| `crates/recoverx-devices/src/windows.rs` | 99 | `serial: None` — Phase 2: WMI |
| `crates/recoverx-storage/src/lib.rs` | 7 | Comment: ForensicImage is Phase 2 placeholder |
| `crates/recoverx-storage/src/physical_device.rs` | 63 | `model: None` — Phase 2: ioctl / WMI |
| `crates/recoverx-storage/src/physical_device.rs` | 64 | `serial: None` — Phase 2: ioctl / WMI |
| `crates/recoverx-storage/src/physical_device.rs` | 147 | Sector size hardcoded 512 — Phase 2: 4Kn ioctl |
| `crates/recoverx-storage/src/disk_image.rs` | 35 | Comment: E01 extension handling Phase 2 |
| `crates/recoverx-storage/src/forensic_image.rs` | 1 | Entire file: Phase 2 module header |
| `crates/recoverx-storage/src/forensic_image.rs` | 26 | Phase 2: EWF / AFF4 parsing |
| `crates/recoverx-storage/src/forensic_image.rs` | 36 | Returns error: "coming in Phase 2" |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 4–5 | Module comment: all tasks are Phase 1 stubs |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 97–100 | `start_scan` comment: Phase 1 stub |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 268 | `run_task` comment: Phase 1 stub |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 290–291 | All 10 match arms: stub, Phase 2 |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 294 | `SourceValidation` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 298 | `PartitionDetection` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 302 | `FilesystemDetection` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 306 | `FilesystemMetadataAnalysis` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 310 | `DeletedFileAnalysis` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 314 | `FileCarving` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 318 | `ResultNormalization` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 322 | `DuplicateDetection` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 326 | `ConfidenceScoring` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/orchestrator.rs` | 330 | `IndexResults` — stub (Phase 1) |
| `crates/recoverx-orchestrator/src/worker.rs` | 83 | WorkerPool comment: carving logic Phase 2 |
| `crates/recoverx-orchestrator/src/worker.rs` | 133 | `files_found: vec![]` — Phase 2 carving output |
| `recoverx-cli/src/main.rs` | 255 | `file_carving` mode uses `deep` config as placeholder |
| `recoverx-cli/src/main.rs` | 275 | Prints "Phase 1 — engine stubs" |

### TypeScript / TSX Source Files

| File | Line | Description |
|------|------|-------------|
| `src/screens/ResultsScreen.tsx` | 1 | Comment: "Phase 2 placeholder" |
| `src/screens/ResultsScreen.tsx` | 18 | UI tag "Coming in Phase 2" |
| `src/screens/ResultsScreen.tsx` | 21 | Description of Phase 2 results browser |
| `src/screens/RecoveryScreen.tsx` | 1 | Comment: "Phase 2 placeholder" |
| `src/screens/RecoveryScreen.tsx` | 18 | UI tag "Coming in Phase 2" |
| `src/screens/RecoveryScreen.tsx` | 21 | Description of Phase 2 recovery manager |
| `src/screens/ReportsScreen.tsx` | 1 | Comment: "Phase 2 placeholder" |
| `src/screens/ReportsScreen.tsx` | 12 | UI tag "Coming in Phase 2" |
| `src/screens/ReportsScreen.tsx` | 15 | Description of Phase 2 report engine |
| `src/screens/SettingsScreen.tsx` | 37 | "Coming in Phase 2" settings controls |
| `src/screens/ScanningScreen.tsx` | 65 | Comment: Phase 1 stubs complete quickly |
| `src/screens/ScanningScreen.tsx` | 222 | Stage label shows "(Phase 1 stub)" |
| `src/screens/HomeScreen.tsx` | 101 | Notice: engines added in Phase 2 |
| `src/screens/ScanSetupScreen.tsx` | 105 | HTML placeholder attribute (non-functional) |

---

## 4. Tauri IPC Chain Status

### Registered Commands (11 total)

```
lib.rs invoke_handler registers:
  list_devices        → commands/devices.rs::list_devices
  get_device_info     → commands/devices.rs::get_device_info
  create_session      → commands/sessions.rs::create_session
  list_sessions       → commands/sessions.rs::list_sessions
  load_session        → commands/sessions.rs::load_session
  delete_session      → commands/sessions.rs::delete_session
  start_scan          → commands/scan.rs::start_scan
  pause_scan          → commands/scan.rs::pause_scan
  resume_scan         → commands/scan.rs::resume_scan
  cancel_scan         → commands/scan.rs::cancel_scan
  get_app_info        → commands/app.rs::get_app_info
```

### Frontend ↔ Backend Mapping

All 11 commands are correctly wired in `src/lib/api.ts` via `invoke()`. The TypeScript types match the Rust structs (verified by field-level inspection):

- `DeviceInfo` ↔ `DeviceInfo` ✅
- `ScanSession` ↔ `ScanSession` ✅
- `CreateSessionRequest` ↔ `CreateSessionRequest` ✅
- `AppInfo` ↔ `AppInfo` ✅

### IPC Chain Assessment

| Command | Status | Notes |
|---------|--------|-------|
| `get_app_info` | ✅ Functional | Returns hardcoded "Phase 1" |
| `list_devices` | ✅ Functional | Calls platform_provider; returns real data or error message |
| `get_device_info` | ✅ Functional | Calls platform_provider |
| `create_session` | ✅ Functional | Writes to SQLite, returns ScanSession |
| `list_sessions` | ✅ Functional | SQLite query |
| `load_session` | ✅ Functional | SQLite query by UUID |
| `delete_session` | ✅ Functional | SQLite DELETE |
| `start_scan` | ✅ Functional (stub engine) | Runs 10-task pipeline; all tasks immediately return Ok(()) |
| `pause_scan` | ✅ Functional | Marks session Paused in DB, emits event |
| `resume_scan` | ✅ Functional | Verifies identity, marks Running |
| `cancel_scan` | ⚠️ BUGGY | Marks session cancelled in memory but never persists to DB (see §15) |

### Event Forwarding Gap

The `EventBus` (broadcast channel) publishes events on the Rust side, but **there is no Tauri event forwarding to the frontend**. The React `ScanningScreen` compensates by polling `load_session` every second, but real-time progress events (`ScanProgress`, `FileFound`, etc.) are never delivered to the UI. This is sufficient for Phase 1 stubs but will become a critical gap when real engines run.

---

## 5. Device Enumeration Status

### macOS (Primary Development Platform)

**Implementation:** `diskutil list` → parse `/dev/diskN` paths → `diskutil info <path>` per disk.

**What works:**
- Lists whole disks (`/dev/disk0`, `/dev/disk1`, etc.)
- Parses: name, size (from bytes parenthetical), sector size, removable flag, filesystem personality, write-protected flag
- Checks accessibility via `File::open()`

**What is missing / may fail:**

1. **Partition filtering:** `is_partition_entry()` detects partition nodes by looking for `'s'` after `"disk"` in the basename. This logic is fragile — it will incorrectly classify `/dev/disk0s1` as a partition (correct) but may misclassify synthetic disk names that contain `'s'` elsewhere. Partition entries are silently skipped.

2. **model and serial are always `None`:** Comments mark them as Phase 2 (IOKit). The macOS `diskutil info` output does contain `Device / Media Name:` and some info about "Disk / Partition / Scheme" but not serial numbers — those require IOKit `kIOSerialNumberString` property access via `IORegistryEntryCreateCFProperty`.

3. **Size parsing:** Relies on `"(N Bytes)"` pattern in `diskutil info`. If diskutil output changes locale or format this will silently return `size_bytes = 0`, and the filesystem metadata stat fallback may also return 0 for block devices.

4. **Filesystem detection:** Uses `File system personality` key from diskutil. On APFS volumes this may return `"APFS"` but on raw synthesised disks it may be absent.

5. **No APFS container / volume awareness:** APFS presents as a container with internal volumes. The current parser sees `/dev/disk1` (the container) and may enumerate it correctly, but APFS internal volumes (`/dev/disk1s1`, etc.) are excluded by the partition filter.

6. **Privilege requirement:** `File::open("/dev/diskN")` on macOS for physical disks requires root. The `is_accessible` flag correctly reflects this, but the UI warning says "Raw device access requires elevated privileges" without offering an authorization prompt.

**Verdict:** Returns real data. Functional for listing disks. Insufficient model/serial data for forensic reports. APFS volume enumeration incomplete.

### Linux

**Implementation:** `/sys/block/` entries → sysfs attributes.

**What works:** size (sectors × 512), sector size, removable flag, model (sysfs), serial (sysfs), device type classification.

**What is missing:** `filesystem: None` (blkid not called — Phase 2). Serial from sysfs usually requires root.

### Windows

**Implementation:** Probe `\\.\PhysicalDrive0..15` via `CreateFileW`.

**Critical issue:** `size_bytes` is hardcoded to `0`. The comment marks `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` as Phase 2. Any code path that reads source size from a Windows `DeviceInfo` will get 0, causing `SourceIdentity` comparisons and progress calculations to fail.

---

## 6. StorageSource Status

### Trait Definition (`source.rs`)

Fully designed. Read-only contract enforced at type level. Key methods:
- `read(offset, length, buf)` — primary interface
- `size()`, `sector_size()`, `metadata()`
- `read_sector()`, `read_sectors()` — convenience helpers
- `identity()` — produces `SourceIdentity` for resume verification

### PhysicalDeviceSource (`physical_device.rs`)

- Opens device `O_RDONLY` (correct).
- Size detection: seek-to-end first, then metadata len, then macOS `DKIOCGETBLOCKCOUNT` ioctl, then Linux `BLKGETSIZE64` ioctl.
- Returns `InsufficientPrivileges` on `PermissionDenied` (correct).
- `model` and `serial` always `None` — Phase 2.
- Sector size hardcoded 512 — Phase 2.
- **No read-ahead buffering** — each `read()` call issues a kernel syscall. For production use, a buffered/cached layer would be needed.

### DiskImageSource (`disk_image.rs`)

- Fully functional for RAW/DD/IMG/BIN.
- Computes SHA-256 of first 64 KiB on open (partial hash for identity).
- In-memory backend for tests.
- 5 passing unit tests.
- E01/AFF extension produces a warning log but proceeds (useful for mislabelled images).

### PartitionSource (`partition.rs`)

- Generic over any `StorageSource`.
- Offset arithmetic correct; bounds enforced on construction and reads.
- 3 passing unit tests.

### ForensicImageSource (`forensic_image.rs`)

- **ALWAYS RETURNS ERROR.** `open()` returns `UnsupportedImageFormat` unconditionally.
- `StorageSource` methods also return errors.
- This is intentional per Phase 1 design, but callers must handle the error.

---

## 7. RecoveryOrchestrator Status

### What is implemented (structural shell):

- `new(data_dir)` — creates data dir, opens SQLite DB, creates `EventBus`.
- `create_session()` — builds `ScanSession`, persists to DB, logs.
- `list_sessions()` / `load_session()` / `delete_session()` — DB delegation.
- `start_scan()` — verifies source identity, checks session state, marks Running, emits `ScanStarted`, iterates pipeline, emits `ScanCompleted`.
- `pause_scan()` / `resume_scan()` — state transitions with identity verification on resume.
- `subscribe()` — returns broadcast receiver for `RecoverXEvent`.

### What is NOT implemented (all 10 pipeline task bodies):

Every `run_task()` match arm does exactly this:
```rust
tracing::info!("X — stub (Phase 1)");
Ok(())
```

The tasks complete in microseconds with zero output. `session.files_found` remains 0. `session.processed_bytes` remains 0. No events are emitted during tasks (only TaskStateChanged before/after).

### WorkerPool (`worker.rs`)

Infrastructure is sound:
- Bounded `crossbeam-channel` work/result queues
- Atomic `ProgressCounters`
- Cancellation `AtomicBool`
- Worker threads loop on `work_rx.recv()`

The worker body only increments `bytes_processed` counter and returns empty `files_found: vec![]`. The pool is **not yet connected to the orchestrator** — `start_scan()` uses the `run_task()` match directly and never instantiates a `WorkerPool`.

### 3 Passing Integration Tests (orchestrator.rs)

- `create_and_load_session` — creates a session and verifies it loads.
- `start_scan_runs_pipeline` — runs all 10 stub tasks, verifies `Completed` status.
- `identity_mismatch_rejects_start` — verifies wrong-identity rejection.

All 3 tests pass (stubs complete immediately). These tests will need to be expanded when real engines are added.

---

## 8. Filesystem Engines

**None exist.** No crate `recoverx-fs`, `recoverx-ntfs`, `recoverx-fat`, or similar is present anywhere in the workspace.

The pipeline has placeholders for three filesystem-related tasks:

| Task | Expected behaviour | Current behaviour |
|------|--------------------|-------------------|
| `PartitionDetection` | Parse MBR/GPT partition table from sector 0/1 | Stub — no-op |
| `FilesystemDetection` | Identify FS type on each partition (FAT32/NTFS/exFAT/ext4/APFS/HFS+) | Stub — no-op |
| `FilesystemMetadataAnalysis` | Walk directory tree, recover inodes/MFT entries | Stub — no-op |

For Phase 2, the following Rust crates could be integrated (none are in Cargo.toml yet):
- `gpt` crate — GPT partition parsing
- `mbrman` crate — MBR partition parsing
- `fatfs` crate — FAT12/FAT16/FAT32 reading
- `ntfs` crate — NTFS MFT parsing
- Custom APFS reader (no mature open-source Rust APFS crate exists as of 2026)

---

## 9. File Carving Engine

**Does not exist.** No signature table, no header/footer scanner, no carving logic anywhere in the codebase.

The `FileCarving` pipeline task stub:
```rust
TaskType::FileCarving => {
    tracing::info!("File carving — stub (Phase 1)");
    Ok(())
}
```

The `WorkerPool` was designed to host carving workers but the worker body is:
```rust
let result = WorkResult {
    sector_offset: item.sector_offset,
    files_found: vec![], // Phase 2: carving engine output
    bad_sector: false,
};
```

For Phase 2, a file carving engine needs:
1. A signature database (file magic bytes — JPEG `FF D8 FF`, PNG `89 50 4E 47`, PDF `%PDF`, etc.)
2. A sector-by-sector scanner feeding the `WorkerPool`
3. File reconstruction logic (header → footer or fixed-size)
4. Integration with `ConfidenceScoring` based on intact headers/footers

---

## 10. Results Browser

**Status: Phase 2 placeholder (UI only).**

`src/screens/ResultsScreen.tsx`:
```tsx
// Results Screen — Phase 2 placeholder
export function ResultsScreen() {
  return (
    <div>
      <h2>Recovery Results</h2>
      <div className="empty-state">
        <h3>No recovery results yet.</h3>
      </div>
      <div className="coming-soon">
        <span className="tag">Coming in Phase 2</span>
        <h3>Results Browser</h3>
        <p>Phase 2 will display recovered files grouped by type, with confidence scores,
           preview thumbnails, and filtering by category, size, and date.</p>
      </div>
    </div>
  );
}
```

No backend API for results exists. The SQLite schema (`scan_sessions` table) has no `recovered_files` table. The `FileFoundEvent` struct exists in `recoverx-core/src/events.rs` but is never emitted.

---

## 11. Recovery Manager

**Status: Phase 2 placeholder (UI only).**

`src/screens/RecoveryScreen.tsx` — empty state + "Coming in Phase 2" notice.

No Tauri command for file recovery exists. No `recoverx-recovery` crate exists.

The Phase 2 description (from the screen) specifies:
- Destination validation (uses `SourceWriteGuard` — already implemented in core)
- Filename sanitisation (`sanitise_filename` — already implemented in core)
- Collision handling
- Atomic writes
- SHA-256 verification
- Recovery logging

The security primitives in `recoverx-core/src/security.rs` are ready to support this. The recovery manager itself needs to be built.

---

## 12. Report Engine

**Status: Phase 2 placeholder (UI only).**

`src/screens/ReportsScreen.tsx` — "Coming in Phase 2" notice.

No Tauri command for reports exists. No report crate exists.

The Phase 2 description specifies: HTML, JSON, and CSV output formats covering case ID, source info, scan mode, timing, files discovered/recovered, methods, confidence, hashes, bad sectors.

---

## 13. SQLite Schema

### Current Schema (migration v1 only)

```sql
CREATE TABLE IF NOT EXISTS scan_sessions (
    id                  TEXT PRIMARY KEY NOT NULL,
    source_identity     TEXT NOT NULL,       -- JSON blob: SourceIdentity
    source_size_bytes   INTEGER NOT NULL,
    sector_size         INTEGER NOT NULL,
    configuration       TEXT NOT NULL,       -- JSON blob: ScanConfiguration
    status              TEXT NOT NULL,       -- String: Created|Running|Paused|Completed|Failed|Cancelled
    last_offset         INTEGER NOT NULL DEFAULT 0,
    processed_bytes     INTEGER NOT NULL DEFAULT 0,
    files_found         INTEGER NOT NULL DEFAULT 0,
    bad_sectors         INTEGER NOT NULL DEFAULT 0,
    engine_state        TEXT,               -- JSON, nullable: opaque engine checkpoint
    last_error          TEXT,
    created_at          TEXT NOT NULL,      -- RFC3339
    updated_at          TEXT NOT NULL,      -- RFC3339
    started_at          TEXT,              -- RFC3339, nullable
    completed_at        TEXT              -- RFC3339, nullable
);

CREATE INDEX IF NOT EXISTS idx_sessions_status ON scan_sessions(status);
CREATE INDEX IF NOT EXISTS idx_sessions_created ON scan_sessions(created_at DESC);

CREATE TABLE schema_version (version INTEGER NOT NULL);
-- Seed row: INSERT INTO schema_version (version) VALUES (1)
```

### Database Settings

- WAL journal mode (set via `PRAGMA journal_mode = WAL`)
- Foreign keys enabled (`PRAGMA foreign_keys = ON`)
- Path: `<app_data_dir>/sessions.db`

### What Is Missing From the Schema

| Missing Table | Purpose | Required For |
|---------------|---------|--------------|
| `recovered_files` | Individual file records found during scan | Results browser, recovery manager |
| `partitions` | Partition table entries detected per session | PartitionDetection task |
| `bad_sectors` | Per-sector error log | Forensic reports |
| `recovery_log` | Per-file write records with SHA-256 | Recovery audit trail |
| `carving_signatures` | Configurable file signatures | File carving engine |
| `report_exports` | Generated report history | Report engine |

The `engine_state` column (opaque JSON) is the only resume checkpoint mechanism. For real engines, a dedicated `engine_checkpoints` table would be cleaner.

---

## 14. Security Layer Status

### SourceWriteGuard (`recoverx-core/src/security.rs`)

- **Implemented and tested.** Registers source paths as protected. Rejects writes to protected paths or any subpath.
- 9 unit tests cover: normal filenames, slash rejection, empty rejection, dot-dot rejection, null byte rejection, colon replacement, guard with registered path, guard with unregistered path, destination-is-source rejection, path traversal detection.

### Issues / Gaps

1. **`SourceWriteGuard` is not wired into the orchestrator or Tauri commands.** It exists in `recoverx-core` but is never instantiated in `RecoveryOrchestrator`, `AppState`, or any command handler. When file recovery is implemented in Phase 2, callers must explicitly create and use a `SourceWriteGuard`, or it provides no protection.

2. **`cancel_scan` does not persist.** See §15.

3. **No CSP enforcement in dev mode.** `tauri.conf.json` sets a strict CSP (`default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'`), but the inline style props throughout the React components (`style={{ ... }}`) will be blocked by this CSP in a strict browser environment. Tauri's WebView may be more permissive by default, but this should be validated.

4. **No privilege escalation UI.** The app requires elevated privileges for raw device access. There is no `sudo` prompt, `osascript` AuthorizationRef, or similar mechanism. Users must launch the app with `sudo` externally.

5. **`path_traversal` guard has a logic gap.** In `assert_no_path_traversal`, when the resolved path is relative it is not checked against `base`. Only absolute paths that don't start with `base` trigger an error. A relative resolved path with no `..` components would pass silently.

---

## 15. Known Bugs and Logic Gaps

### Bug 1: `cancel_scan` Does Not Persist

**File:** `src-tauri/src/commands/scan.rs`, lines 76–93

```rust
pub async fn cancel_scan(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    // ...
    let mut session = orchestrator.load_session(&id).map_err(|e| e.to_string())?;
    session.mark_cancelled();
    // Persist cancelled state via update_session in the db
    // (orchestrator doesn't expose update_session directly — use delete/recreate or add method)
    // For now, log the cancellation
    tracing::info!(session_id = %id, "Scan cancelled by user");
    Ok(())
}
```

`mark_cancelled()` is called on a local copy of the session. The change is **never written to the database**. After cancellation, `load_session` will return the old status (Running or Paused). The orchestrator needs an `update_session` method exposed publicly, or `cancel_scan` needs to call the existing private `persist_session` indirectly.

**Fix required:** Add `pub fn update_session(&self, session: &ScanSession) -> Result<()>` to `RecoveryOrchestrator` and call it in `cancel_scan`.

### Bug 2: `ScanningScreen` Progress Animation Is Disconnected From Reality

The UI animates stage progress every 300ms regardless of actual backend state:
```typescript
setCurrentStage((s) => Math.min(s + 1, PIPELINE_STAGES.length - 1));
```

Since Phase 1 stubs complete in microseconds, the real scan finishes before the animation reaches stage 3. The `session.status` will be `"completed"` on the first poll (1s interval), but the UI may show the animation still running. This is cosmetic now but needs event-driven updates in Phase 2.

### Bug 3: `is_partition_entry` False Positive Risk

```rust
fn is_partition_entry(path: &str) -> bool {
    path.contains('s') && {
        let basename = path.trim_start_matches("/dev/");
        basename.contains('s') && basename.starts_with("disk")
    }
}
```

This returns `true` for any path that contains `'s'` after `/dev/disk`, including `/dev/disks0` (hypothetical), or `/dev/disk0s1` (correct). The condition `path.contains('s')` is always true for strings like `/dev/disk0s1` (correct) but also `/dev/disk_ssd0` if such a path existed. The regex should be more explicit: `^/dev/disk\d+s\d+$`.

### Logic Gap: `start_scan` Takes `source_identity` From Frontend

In `commands/scan.rs::start_scan`, the `SourceIdentity` is reconstructed from the frontend-supplied `source_path` and `source_size_bytes`:
```rust
let identity = SourceIdentity {
    label: source_path.clone(),
    path: source_path,
    size_bytes: source_size_bytes,
    sector_size,
    model: None,
    serial: None,
    ...
};
```

The frontend sends `session.source_identity.path` and `session.source_size_bytes`. This means the identity verification in `orchestrator.start_scan()` compares a frontend-supplied identity against the stored one. A malicious or buggy frontend could supply `size_bytes: 0` and bypass the identity check (since the stored session's `source_size_bytes` is also 0 when created from a non-existent device path). This is low risk in Phase 1 but should be re-examined when real device access is added.

### Logic Gap: No `recovered_files` Table

`session.files_found` is a counter in `scan_sessions` but individual file records have no storage. The `FileFoundEvent` struct exists but is never emitted and there's no target table. Phase 2 must add the table before results can be shown.

---

## 16. Dependency Inventory

### Rust Workspace Dependencies (Cargo.toml)

| Crate | Version | Purpose |
|-------|---------|---------|
| tokio | 1.40 (full) | Async runtime |
| serde | 1.0 (derive) | Serialization |
| serde_json | 1.0 | JSON encoding |
| thiserror | 1.0 | Error derive macro |
| anyhow | 1.0 | Error context (CLI/logging) |
| tracing | 0.1 | Structured logging |
| tracing-subscriber | 0.3 (env-filter, json) | Log output |
| rusqlite | 0.31 (bundled) | SQLite (bundled = static link) |
| uuid | 1.10 (v4, serde) | Session/task IDs |
| chrono | 0.4 (serde) | Timestamps |
| sha2 | 0.10 | SHA-256/SHA-512 hashing |
| md-5 | 0.10 | MD5 (present in core, unused) |
| hex | 0.4 | Hex encoding for hashes |
| crossbeam-channel | 0.5 | Bounded worker queue |
| once_cell | 1.19 | Lazy statics |
| libc | 0.2 | ioctl on macOS/Linux |
| winapi | 0.3 (various features) | Windows device access |
| tauri | 2.0 | GUI framework |
| tauri-plugin-shell | 2.0 | Shell plugin (declared, not visibly used) |
| clap | 4.5 (derive) | CLI argument parsing |
| tempfile | 3.12 (dev) | Test temp dirs |

**Notable absence:** No filesystem parsing crates (fatfs, ntfs, gpt, etc.), no EWF/E01 library.

**Potential unused dep:** `md-5` is declared in `recoverx-core/Cargo.toml` but no MD5 hashing is performed in any source file. Can be removed unless planned for weak-hash comparison in duplicate detection.

### Frontend Dependencies (package.json)

| Package | Version | Purpose |
|---------|---------|---------|
| @tauri-apps/api | ^2.0.0 | Tauri IPC (invoke, event) |
| @tauri-apps/plugin-shell | ^2.0.0 | Shell plugin |
| react | ^19.2.8 | UI framework |
| react-dom | ^19.2.8 | DOM renderer |
| react-router-dom | ^7.7.1 | Client-side routing |
| vite | ^8.3.0 | Build tool |
| typescript | ~6.0.2 | Type checking |
| @vitejs/plugin-react | ^6.1.1 | React + Vite integration |

---

## 17. All Missing Implementations

Listed in approximate dependency order:

### Backend (Rust)

1. **`update_session` on `RecoveryOrchestrator`** — Needed to fix `cancel_scan` bug.
2. **`cancel_scan` fix** — Call `persist_session` after `mark_cancelled()`.
3. **`SourceWriteGuard` wiring** — Instantiate in `AppState` and use in scan commands.
4. **Privilege escalation** — `osascript` / `AuthorizationRef` on macOS, UAC on Windows.
5. **IOKit device model/serial (macOS)** — `IORegistryEntryCreateCFProperty` for `kIOSerialNumberString` and `kIOPropertyProductKey`.
6. **IOCTL device size/sector (Windows)** — `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`.
7. **WMI model/serial (Windows)** — WMI `Win32_DiskDrive` query.
8. **blkid filesystem detection (Linux)** — Detect filesystem type on block devices.
9. **4Kn sector size detection** — Use `DKIOCGETBLOCKSIZE` (macOS) or `BLKSSZGET` (Linux).
10. **`recovered_files` SQLite table** — Schema migration v2.
11. **`partitions` SQLite table** — Schema migration v2 or v3.
12. **`bad_sectors` SQLite table** — Schema migration.
13. **`recovery_log` SQLite table** — Schema migration.
14. **Partition detection engine** — MBR (offset 446) + GPT (LBA 1) parser. Use `gpt`/`mbrman` crates.
15. **Filesystem detection** — Magic byte identification at partition start (FAT, NTFS, ext4, APFS, HFS+).
16. **FAT32 filesystem engine** — Directory tree walk, deleted entry recovery (`0xE5` mark).
17. **NTFS filesystem engine** — MFT parsing, `$LogFile` analysis, orphan inode recovery.
18. **ext4 filesystem engine** — Inode table walk, journal analysis (Linux only likely).
19. **APFS filesystem engine** — Container/volume structure (macOS only; no public Rust crate).
20. **HFS+ filesystem engine** — Catalog B-tree walk (legacy macOS).
21. **exFAT filesystem engine** — FAT chain parsing.
22. **File carving signature database** — JPEG, PNG, PDF, ZIP, DOCX, MP4, MP3, etc.
23. **File carving scanner** — Sector-by-sector header/footer search feeding `WorkerPool`.
24. **File reconstruction** — Header→footer or header+size-field for fixed-format files.
25. **WorkerPool integration** — Connect `WorkerPool` to `run_task(FileCarving)`.
26. **`DeletedFileAnalysis` engine** — Recover `0xE5`-marked FAT entries; orphan MFT records.
27. **Result normalization** — Dedup file records from multiple analysis passes.
28. **Duplicate detection** — SHA-256 content hash comparison across found files.
29. **Confidence scoring** — Heuristic scoring: intact header, intact footer, known extension, filesystem metadata consistency.
30. **Index results** — Write `recovered_files` rows to SQLite.
31. **Event forwarding to frontend** — Tauri `app.emit()` or window event for `RecoverXEvent`.
32. **`RecoveryManager`** — Write files to destination with `SourceWriteGuard`, SHA-256 verify, collision handling, recovery log.
33. **`ResultsScreen`** — List/filter/preview recovered files via new IPC commands.
34. **`RecoveryScreen`** — Select and recover files via `RecoveryManager` IPC.
35. **`ReportEngine`** — HTML/JSON/CSV export; wire to `ReportsScreen`.
36. **Settings persistence** — Store defaults in SQLite or app config file.
37. **Forensic image source (E01/AFF4)** — `ForensicImageSource::open()` implementation.
38. **`path_traversal` guard fix** — Handle relative path case in `assert_no_path_traversal`.
39. **`is_partition_entry` regex fix** — Replace substring check with explicit pattern.
40. **Remove unused `md-5` dep** — Or wire it for duplicate detection weak hash.

---

## 18. Recommended Implementation Order for Phase 2

The following order minimises blockers and delivers testable value early.

### Sprint 1 — Critical Fixes and Infrastructure (1–2 weeks)

1. Fix `cancel_scan` persistence bug (30 min).
2. Add `RecoveryOrchestrator::update_session()` public method (30 min).
3. Fix `is_partition_entry` regex (1 hour).
4. Fix `assert_no_path_traversal` relative path gap (1 hour).
5. Wire `SourceWriteGuard` into `AppState` (2 hours).
6. Add schema migration v2: `recovered_files` table (1 day).
7. Add schema migration v3: `partitions`, `bad_sectors`, `recovery_log` tables (1 day).
8. Remove unused `md-5` dependency (5 min).

### Sprint 2 — Partition and Filesystem Detection (2–3 weeks)

9. Implement `PartitionDetection` engine: MBR + GPT parsing using `gpt`/`mbrman` crates.
10. Implement `FilesystemDetection`: magic byte identification at partition start.
11. Add `PartitionDetectedEvent` and `FilesystemDetectedEvent` emission.
12. Persist partition table to `partitions` table.
13. Implement `FilesystemMetadataAnalysis` for FAT32 (most widely recoverable format).
14. Implement `FilesystemMetadataAnalysis` for NTFS (most common on Windows targets).
15. Write filesystem metadata to `recovered_files` table.
16. Add Tauri event forwarding: subscribe to `EventBus` in `setup()`, re-emit via `app.emit()`.
17. Update `ScanningScreen` to use Tauri events instead of polling.

### Sprint 3 — File Carving (2–3 weeks)

18. Create `crates/recoverx-carving` crate.
19. Implement signature database (JPEG, PNG, PDF, ZIP, DOCX, MP4, AVI, MP3, WAV, GIF, BMP).
20. Implement sector-by-sector scanner feeding existing `WorkerPool`.
21. Implement file reconstruction (header→footer, header+size-field).
22. Wire `WorkerPool` into `run_task(FileCarving)`.
23. Implement `DeletedFileAnalysis` for FAT (0xE5 entries) and NTFS (orphan MFT records).
24. Implement `ResultNormalization`: merge carving + filesystem results, dedup by offset/hash.

### Sprint 4 — Post-Processing and Results (1–2 weeks)

25. Implement `DuplicateDetection`: SHA-256 of reconstructed file content.
26. Implement `ConfidenceScoring`: heuristic rules per file type.
27. Implement `IndexResults`: write final set to `recovered_files`.
28. Implement `list_recovered_files` and `get_recovered_file` Tauri commands.
29. Build `ResultsScreen`: file table with type, size, confidence, preview.
30. Implement `filter_results` Tauri command (by category, confidence, size, date).

### Sprint 5 — Recovery and Reports (1–2 weeks)

31. Implement `RecoveryManager` crate with destination validation, atomic writes, SHA-256 verify, `recovery_log` writes.
32. Implement `recover_file` and `recover_files` Tauri commands.
33. Build `RecoveryScreen`: file selection, destination picker, progress.
34. Implement `ReportEngine`: HTML + JSON + CSV generators.
35. Implement `generate_report` Tauri command.
36. Build `ReportsScreen`: report list, generate button, export.

### Sprint 6 — Platform Hardening (1 week)

37. macOS: IOKit model/serial via `core-foundation`/`io-kit-sys` crates.
38. macOS: Authorization Services privilege escalation prompt.
39. Linux: `blkid` integration for filesystem detection on raw devices.
40. Windows: `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` for size.
41. Windows: WMI model/serial via `wmi` crate.
42. All: 4Kn sector size detection.
43. Forensic image: E01 via `ewf` or custom reader.
44. Settings: persistence via SQLite or `tauri-plugin-store`.

---

## Summary Table

| Dimension | Phase 1 Status |
|-----------|---------------|
| Architecture | ✅ Well-structured, layered, no circular deps |
| Error handling | ✅ Comprehensive enum, typed errors |
| Event system | ✅ Designed and implemented; not forwarded to UI |
| Security primitives | ✅ Implemented; not wired into operations |
| SQLite persistence | ✅ Sessions only; no file records |
| Device enumeration | ✅ Real data; model/serial/size gaps per platform |
| Storage abstraction | ✅ Physical + image + partition; forensic = stub |
| Pipeline orchestration | ✅ Structure + events; all 10 tasks are no-ops |
| Worker pool | ✅ Infrastructure; carving logic absent |
| Tauri IPC | ✅ 11 commands wired; cancel_scan buggy |
| Frontend | ✅ 5 functional screens; 3 placeholder screens |
| CLI | ✅ Functional (devices, sessions, scan) |
| Tests | ✅ 25+ unit/integration tests; all Phase 1 scope |
| **Actual recovery capability** | **❌ Zero** — no file is ever found or recovered |

**Bottom line:** RecoverX Phase 1 is a production-quality foundation — the architecture, persistence, IPC plumbing, security primitives, and UI scaffold are all solid. The application compiles, launches, creates sessions, and "runs" scans (all tasks complete as no-ops). No data is actually recovered. Phase 2 must implement all 10 pipeline task bodies starting from partition detection and filesystem parsing before the product has any recovery capability.
