import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, formatBytes } from "../lib/api";
import type { CommonLocation, CreateSessionRequest, DeviceInfo } from "../lib/api";

interface LocationState {
  device?: DeviceInfo;
  mode?: "quick" | "deep" | "file_carving";
}

const MODES = [
  {
    id: "quick",
    label: "Quick Scan",
    icon: "⚡",
    description: "Scans filesystem metadata and deleted directory entries. Fast — typically completes in minutes.",
    config: {
      enable_filesystem_analysis: true,
      enable_deleted_file_recovery: true,
      enable_file_carving: false,
      enable_duplicate_detection: false,
      enable_forensic_hashing: false,
    },
  },
  {
    id: "deep",
    label: "Deep Scan",
    icon: "🔬",
    description: "Full analysis: deleted entries + file carving of unallocated space. Most thorough.",
    config: {
      enable_filesystem_analysis: true,
      enable_deleted_file_recovery: true,
      enable_file_carving: true,
      enable_duplicate_detection: true,
      enable_forensic_hashing: true,
    },
  },
  {
    id: "file_carving",
    label: "File Carving",
    icon: "🪛",
    description: "Signature-based raw sector scan. Use when the filesystem is damaged or the drive was formatted.",
    config: {
      enable_filesystem_analysis: false,
      enable_deleted_file_recovery: false,
      enable_file_carving: true,
      enable_duplicate_detection: false,
      enable_forensic_hashing: false,
    },
  },
] as const;

const TIMELINE_PRESETS = [
  { label: "Last 24 hours", days: 1 },
  { label: "Last 7 days",   days: 7 },
  { label: "Last 30 days",  days: 30 },
  { label: "Last 90 days",  days: 90 },
  { label: "All time",      days: 0 },
] as const;

export function ScanSetupScreen() {
  const navigate = useNavigate();
  const location = useLocation();
  const locState = (location.state as LocationState) ?? {};

  const [locations, setLocations] = useState<CommonLocation[]>([]);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [loading, setLoading] = useState(true);

  // Selected source
  const [sourcePath, setSourcePath] = useState(locState.device?.path ?? "");
  const [sourceLabel, setSourceLabel] = useState(locState.device?.name ?? "");
  const [sourceDevice, setSourceDevice] = useState<DeviceInfo | null>(locState.device ?? null);
  const [filterPrefix, setFilterPrefix] = useState<string | null>(null);
  const [manualPath, setManualPath] = useState("");

  // Timeline
  const [presetDays, setPresetDays] = useState(30);
  const [customFrom, setCustomFrom] = useState("");
  const [customTo, setCustomTo] = useState("");
  const [useCustom, setUseCustom] = useState(false);

  // Mode
  const [mode, setMode] = useState<string>(locState.mode ?? "quick");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      api.getCommonLocations().catch(() => [] as CommonLocation[]),
      api.listDevices().catch(() => ({ devices: [] as DeviceInfo[], error: null })),
    ]).then(([locs, devRes]) => {
      setLocations(locs);
      setDevices(devRes.devices ?? []);
      setLoading(false);
    });
  }, []);

  const timelineTimestamps = () => {
    if (useCustom) {
      return {
        after: customFrom ? Math.floor(new Date(customFrom).getTime() / 1000) : null,
        before: customTo ? Math.floor(new Date(customTo + "T23:59:59").getTime() / 1000) : null,
      };
    }
    if (presetDays === 0) return { after: null, before: null };
    return { after: Math.floor(Date.now() / 1000) - presetDays * 86400, before: null };
  };

  const selectedMode = MODES.find((m) => m.id === mode) ?? MODES[0];

  const selectLocation = (loc: CommonLocation) => {
    setSourcePath(loc.path);
    setSourceLabel(loc.label);
    setSourceDevice(null);
    setFilterPrefix(loc.filter_prefix);
    setManualPath("");
  };

  const selectDevice = (dev: DeviceInfo) => {
    if (!dev.is_accessible) return;
    setSourcePath(dev.path);
    setSourceLabel(dev.name);
    setSourceDevice(dev);
    setFilterPrefix(null);
    setManualPath("");
  };

  const handleManualChange = (val: string) => {
    setManualPath(val);
    setSourcePath(val);
    setSourceLabel("");
    setSourceDevice(null);
    setFilterPrefix(null);
  };

  const handleStart = async () => {
    const path = sourcePath.trim() || manualPath.trim();
    if (!path) { setError("Please select a source."); return; }

    setCreating(true);
    setError(null);
    const { after, before } = timelineTimestamps();

    try {
      const req: CreateSessionRequest = {
        source_path: path,
        source_size_bytes: sourceDevice?.size_bytes ?? 0,
        sector_size: sourceDevice?.sector_size ?? 512,
        scan_mode: mode,
        ...selectedMode.config,
        deleted_after: after,
        deleted_before: before,
        filter_prefix: filterPrefix,
      };
      const session = await api.createSession(req);
      navigate("/scanning", { state: { session } });
    } catch (err) {
      setError(String(err));
    } finally {
      setCreating(false);
    }
  };

  const timelineSummary = () => {
    if (useCustom) {
      if (customFrom && customTo) return `${customFrom} → ${customTo}`;
      if (customFrom) return `After ${customFrom}`;
      if (customTo) return `Before ${customTo}`;
      return "All time";
    }
    return TIMELINE_PRESETS.find((p) => p.days === presetDays)?.label ?? "All time";
  };

  // Group locations: folder shortcuts vs. trash/all
  const folderLocations = locations.filter((l) => l.filter_prefix !== null);
  const trashLocations  = locations.filter((l) => l.filter_prefix === null);

  const isSelected = (loc: CommonLocation) =>
    sourcePath === loc.path && filterPrefix === loc.filter_prefix;

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "24px" }}>
        <button className="btn btn-ghost" onClick={() => navigate("/devices")}>← Devices</button>
        <div>
          <h2>Scan Setup</h2>
          <p style={{ color: "var(--text-secondary)", fontSize: "0.8125rem", marginTop: "2px" }}>
            Choose what to scan, set a time window, then start recovering.
          </p>
        </div>
      </div>

      {error && <div className="alert alert-error" style={{ marginBottom: "16px" }}>{error}</div>}

      {/* ── STEP 1: Source ── */}
      <div className="card" style={{ marginBottom: "16px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "14px" }}>
          <h3>① Source</h3>
          {sourcePath && (
            <span style={{ fontSize: "0.8125rem", color: "var(--accent-green)" }}>
              ✓ {sourceLabel || sourcePath}
              {filterPrefix && <span style={{ color: "var(--text-muted)" }}> (deleted from {filterPrefix.split("/").pop()})</span>}
            </span>
          )}
        </div>

        {/* Folder shortcuts — all scan Trash */}
        {!loading && folderLocations.length > 0 && (
          <div style={{ marginBottom: "18px" }}>
            <p style={{ fontSize: "0.7rem", color: "var(--text-muted)", marginBottom: "10px", textTransform: "uppercase", letterSpacing: "0.06em" }}>
              Find deleted files from…
            </p>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "8px" }}>
              {folderLocations.map((loc) => (
                <button
                  key={loc.label}
                  onClick={() => selectLocation(loc)}
                  disabled={!loc.exists}
                  title={loc.hint}
                  style={{
                    padding: "14px 10px",
                    background: isSelected(loc) ? "var(--accent-blue)" : "var(--bg-primary)",
                    border: `1px solid ${isSelected(loc) ? "var(--accent-blue)" : "var(--border)"}`,
                    borderRadius: "10px",
                    color: isSelected(loc) ? "#fff" : loc.exists ? "var(--text-primary)" : "var(--text-muted)",
                    cursor: loc.exists ? "pointer" : "not-allowed",
                    opacity: loc.exists ? 1 : 0.45,
                    textAlign: "center",
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "center",
                    gap: "6px",
                    transition: "all 0.12s",
                  }}
                >
                  <span style={{ fontSize: "1.75rem" }}>{loc.icon}</span>
                  <span style={{ fontSize: "0.8125rem", fontWeight: 600 }}>{loc.label}</span>
                </button>
              ))}
            </div>
            <div className="alert alert-warning" style={{ marginTop: "10px", padding: "8px 12px" }}>
              <strong>Note:</strong> These buttons all scan your <strong>Trash</strong> and show files
              that are still there waiting to be permanently deleted. macOS does not record which
              folder files originally came from, so all Trash items are shown.
              <br />
              <strong>For files already emptied from Trash</strong> (permanently deleted), select a
              storage device below — raw disk scanning is required.
            </div>
          </div>
        )}

        {/* Trash / whole-trash options */}
        {!loading && trashLocations.length > 0 && (
          <div style={{ marginBottom: "18px" }}>
            <p style={{ fontSize: "0.7rem", color: "var(--text-muted)", marginBottom: "10px", textTransform: "uppercase", letterSpacing: "0.06em" }}>
              All Trash
            </p>
            <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
              {trashLocations.map((loc) => (
                <button
                  key={loc.label + loc.path}
                  onClick={() => selectLocation(loc)}
                  disabled={!loc.exists}
                  title={loc.hint}
                  style={{
                    padding: "10px 16px",
                    background: isSelected(loc) ? "var(--accent-blue)" : "var(--bg-primary)",
                    border: `1px solid ${isSelected(loc) ? "var(--accent-blue)" : "var(--border)"}`,
                    borderRadius: "8px",
                    color: isSelected(loc) ? "#fff" : loc.exists ? "var(--text-primary)" : "var(--text-muted)",
                    cursor: loc.exists ? "pointer" : "not-allowed",
                    opacity: loc.exists ? 1 : 0.45,
                    display: "flex",
                    alignItems: "center",
                    gap: "8px",
                    fontSize: "0.875rem",
                    fontWeight: 600,
                  }}
                >
                  <span style={{ fontSize: "1.2rem" }}>{loc.icon}</span>
                  {loc.label}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Physical devices */}
        {devices.length > 0 && (
          <div style={{ marginBottom: "16px" }}>
            <p style={{ fontSize: "0.7rem", color: "var(--text-muted)", marginBottom: "6px", textTransform: "uppercase", letterSpacing: "0.06em" }}>
              Storage devices — recover permanently deleted files
            </p>
            <p style={{ fontSize: "0.75rem", color: "var(--text-secondary)", marginBottom: "10px" }}>
              For files deleted and emptied from Trash. Requires Full Disk Access.
            </p>
            <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
              {devices.map((dev) => (
                <button
                  key={dev.path}
                  onClick={() => selectDevice(dev)}
                  style={{
                    padding: "10px 14px",
                    background: sourcePath === dev.path && !filterPrefix ? "var(--accent-blue)" : "var(--bg-primary)",
                    border: `1px solid ${sourcePath === dev.path && !filterPrefix ? "var(--accent-blue)" : dev.is_accessible ? "var(--border)" : "var(--accent-red)44"}`,
                    borderRadius: "8px",
                    color: sourcePath === dev.path && !filterPrefix ? "#fff" : dev.is_accessible ? "var(--text-primary)" : "var(--text-muted)",
                    cursor: dev.is_accessible ? "pointer" : "not-allowed",
                    display: "flex",
                    alignItems: "center",
                    gap: "12px",
                    opacity: dev.is_accessible ? 1 : 0.65,
                    textAlign: "left",
                  }}
                >
                  <span style={{ fontSize: "1.25rem" }}>{dev.is_internal ? "🖥️" : "🔌"}</span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 600, fontSize: "0.875rem", display: "flex", alignItems: "center", gap: "8px" }}>
                      {dev.name}
                      {!dev.is_accessible && (
                        <span style={{ fontSize: "0.7rem", padding: "1px 6px", borderRadius: "4px", background: "#991b1b", color: "#fecaca" }}>
                          Needs Full Disk Access
                        </span>
                      )}
                    </div>
                    <div style={{ fontSize: "0.75rem", opacity: 0.8 }}>
                      {dev.path} · {formatBytes(dev.size_bytes)}
                      {!dev.is_accessible && (
                        <span style={{ color: "var(--accent-amber)" }}>
                          {" "}— Go to System Settings → Privacy & Security → Full Disk Access → add RecoverX
                        </span>
                      )}
                    </div>
                  </div>
                  <span style={{
                    fontSize: "0.65rem", padding: "2px 6px", borderRadius: "4px",
                    background: dev.is_internal ? "#1d4ed855" : "#92400e55",
                    color: dev.is_internal ? "#bfdbfe" : "#fde68a", flexShrink: 0,
                  }}>
                    {dev.is_internal ? "INTERNAL" : "EXTERNAL"}
                  </span>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Manual path */}
        <div>
          <p style={{ fontSize: "0.7rem", color: "var(--text-muted)", marginBottom: "6px", textTransform: "uppercase", letterSpacing: "0.06em" }}>
            Or enter a disk image path manually
          </p>
          <input
            className="form-input mono"
            type="text"
            value={manualPath}
            onChange={(e) => handleManualChange(e.target.value)}
            placeholder="/path/to/backup.img  or  /dev/disk2"
          />
        </div>
      </div>

      {/* ── STEP 2: Timeline ── */}
      <div className="card" style={{ marginBottom: "16px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "12px" }}>
          <h3>② Timeline</h3>
          <span style={{ fontSize: "0.8125rem", color: "var(--accent-green)" }}>{timelineSummary()}</span>
        </div>
        <p style={{ fontSize: "0.8125rem", color: "var(--text-secondary)", marginBottom: "12px" }}>
          Only show files deleted within this window. Files with no timestamp are always included.
        </p>
        {!useCustom && (
          <div style={{ display: "flex", gap: "8px", flexWrap: "wrap", marginBottom: "10px" }}>
            {TIMELINE_PRESETS.map((p) => (
              <button
                key={p.days}
                onClick={() => setPresetDays(p.days)}
                style={{
                  padding: "6px 14px",
                  background: presetDays === p.days ? "var(--accent-blue)" : "var(--bg-primary)",
                  border: `1px solid ${presetDays === p.days ? "var(--accent-blue)" : "var(--border)"}`,
                  borderRadius: "20px",
                  color: presetDays === p.days ? "#fff" : "var(--text-secondary)",
                  cursor: "pointer",
                  fontSize: "0.8125rem",
                  fontWeight: presetDays === p.days ? 600 : 400,
                }}
              >{p.label}</button>
            ))}
          </div>
        )}
        <button
          onClick={() => setUseCustom((v) => !v)}
          style={{ background: "none", border: "none", color: "var(--accent-blue)", cursor: "pointer", fontSize: "0.8125rem", padding: 0 }}
        >
          {useCustom ? "▲ Use preset" : "▼ Custom date range"}
        </button>
        {useCustom && (
          <div style={{ display: "flex", gap: "12px", marginTop: "12px" }}>
            <div style={{ flex: 1 }}>
              <label className="form-label">Deleted after</label>
              <input className="form-input" type="date" value={customFrom} onChange={(e) => setCustomFrom(e.target.value)} />
            </div>
            <span style={{ color: "var(--text-muted)", marginTop: "22px" }}>→</span>
            <div style={{ flex: 1 }}>
              <label className="form-label">Deleted before</label>
              <input className="form-input" type="date" value={customTo} onChange={(e) => setCustomTo(e.target.value)} />
            </div>
          </div>
        )}
      </div>

      {/* ── STEP 3: Scan Mode ── */}
      <div className="card" style={{ marginBottom: "24px" }}>
        <h3 style={{ marginBottom: "14px" }}>③ Scan Mode</h3>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "10px", marginBottom: "10px" }}>
          {MODES.map((m) => (
            <button
              key={m.id}
              onClick={() => setMode(m.id)}
              style={{
                padding: "14px 12px",
                background: mode === m.id ? "var(--accent-blue)" : "var(--bg-primary)",
                border: `1px solid ${mode === m.id ? "var(--accent-blue)" : "var(--border)"}`,
                borderRadius: "8px",
                color: mode === m.id ? "#fff" : "var(--text-secondary)",
                cursor: "pointer",
                fontWeight: 600,
                display: "flex",
                alignItems: "center",
                gap: "8px",
                fontSize: "0.875rem",
              }}
            >
              <span style={{ fontSize: "1.1rem" }}>{m.icon}</span>
              {m.label}
            </button>
          ))}
        </div>
        <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", lineHeight: 1.6 }}>
          {selectedMode.description}
        </p>
      </div>

      <div style={{ display: "flex", gap: "12px", alignItems: "center" }}>
        <button
          className="btn btn-primary btn-lg"
          onClick={handleStart}
          disabled={creating || (!sourcePath.trim() && !manualPath.trim())}
        >
          {creating ? "Starting…" : `▶ Start ${selectedMode.label}`}
        </button>
        <button className="btn btn-ghost btn-lg" onClick={() => navigate(-1)}>Cancel</button>
        {(sourcePath || manualPath) && (
          <span style={{ fontSize: "0.8125rem", color: "var(--text-muted)", marginLeft: "auto" }}>
            {sourceLabel || sourcePath || manualPath} · {timelineSummary()} · {selectedMode.label}
          </span>
        )}
      </div>

      <div className="alert alert-info" style={{ marginTop: "20px" }}>
        <strong>Note:</strong> RecoverX only shows files that are genuinely deleted — either in your Trash,
        or detected as deleted entries on the raw storage device.
        It never lists currently-existing files as "recoverable".
      </div>
    </div>
  );
}
