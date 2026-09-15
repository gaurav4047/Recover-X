# Security Model

## Source Write Protection

`SourceWriteGuard` (in `recoverx-core`) is the central enforcement point:

1. Every source device/image path is registered with `register_source()` before any scan.
2. Before any file is written, `assert_safe_destination(dest)` verifies the destination is not on the same path or a subpath of a registered source.
3. If the check fails, the operation is rejected with `DestinationIsSource` error.
4. The `StorageSource` trait has no write method — writes are **impossible at the type level**.

```
SOURCE_WRITE_BLOCKED is logged if any code path attempts to write to a registered source.
```

## Read-Only Enforcement

- `PhysicalDeviceSource::open()` uses `OpenOptions::new().read(true)` — no write flag.
- `DiskImageSource::open()` uses `OpenOptions::new().read(true)`.
- Partition sources are windowed views over read-only sources.

## Filename Sanitisation

`sanitise_filename()` rejects:
- Empty names
- Names containing `/`, `\`, or null bytes
- `.` and `..`
- Names longer than 255 bytes
- Control characters (ASCII < 32)

Unsafe characters (`< > : " | ? *`) are replaced with `_`.

## Path Traversal Prevention

`assert_no_path_traversal(base, target)` resolves `..` components manually and rejects any path that escapes the base directory.

## Atomic Writes

Recovery writes use a temp-file-then-rename pattern:
1. Write to `.recoverx_tmp_<uuid>` in the destination directory.
2. Rename to the final filename atomically.
3. If any step fails, the temp file is cleaned up.

## SHA-256 Verification

Every recovered file has its SHA-256 computed during the write. The hash is:
- Stored in the `recovery_log` SQLite table.
- Displayed in the Recovery screen.
- Returned to the caller for independent verification.

## Recovered Files Are Untrusted

- Recovered files are never executed.
- Preview is limited to images (displayed in Tauri WebView sandbox), text, and hex dumps.
- EXE, DLL, ELF, Mach-O, shell scripts, and JavaScript files show metadata only.

## No Network Transmission

RecoverX never transmits source data, recovered files, or hashes to any network endpoint. All operations are local.
