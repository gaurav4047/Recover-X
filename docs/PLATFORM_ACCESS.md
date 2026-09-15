# Platform Access

## macOS

### Device Enumeration
Uses `diskutil list` and `diskutil info <path>` to enumerate whole disks. Whole-disk nodes (`/dev/disk0`, `/dev/disk1`) are detected by matching the regex `disk\d+` exactly (no trailing `s` for partition slices).

### Raw Device Access
Opening `/dev/diskN` for reading requires Full Disk Access permission or running as root. Without privileges, `is_accessible = false` is reported and the device is shown with a "No Access" badge.

**Granting access:**
```bash
# Option 1: run with sudo
sudo ./recoverx

# Option 2: grant Full Disk Access in System Preferences
# System Preferences → Privacy & Security → Full Disk Access → add RecoverX
```

### What RecoverX does NOT bypass
- FileVault encryption (encrypted volumes show as unreadable)
- System Integrity Protection (SIP)
- macOS Gatekeeper signing requirements
- APFS encryption

### Internal Disk Detection
`devicelocation: Internal` in `diskutil info` output sets `is_internal = true`. The UI groups devices into INTERNAL and EXTERNAL/REMOVABLE sections.

---

## Linux

### Device Enumeration
Reads `/sys/block/` directory. Loop devices (`loop*`), RAM disks (`ram*`), and zram devices are excluded. Model and serial are read from sysfs (`/sys/block/<dev>/device/model`).

### Raw Device Access
Opening `/dev/sdX` or `/dev/nvme0n1` requires root or membership in the `disk` group.

```bash
sudo ./recoverx
# or
sudo usermod -aG disk $USER  # then log out/in
```

---

## Windows

### Device Enumeration
Probes `\\.\PhysicalDrive0` through `\\.\PhysicalDrive15` by attempting to open each with `GENERIC_READ`. Drives that do not exist return `DeviceNotFound` and enumeration stops.

### Raw Device Access
Requires Administrator privileges.

```
Right-click RecoverX → Run as Administrator
```

### Limitations
- Device size is not yet queried via `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` (returns 0).
- Model and serial number require WMI `Win32_DiskDrive` query (not yet implemented).

---

## Disk Images

RecoverX accepts RAW/DD/IMG disk images as source. No elevated privileges are required to read image files.

```
/path/to/backup.img
/path/to/usb_dump.dd
/path/to/forensic_copy.raw
```

E01/AFF forensic image formats are not currently supported.
