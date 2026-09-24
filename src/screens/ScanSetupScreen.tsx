import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, formatBytes } from "../lib/api";
import type { CommonLocation, CreateSessionRequest, DeviceInfo } from "../lib/api";

interface LocationState {
  device?: DeviceInfo;
  mode?: "quick" | "deep" | "file_carving";
}

// ── Scan modes ────────────────────────────────────────────────────────────────

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
    description: "Full analysis: filesystem + deleted entries + file carving. Most thorough, takes longer.",
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

// ── Component ─────────────────────────────────────────────────────────────────

export function ScanSetupScreen() {
  const navigate = useNavigate();
  const location = useLocation();
  const locState = (location.state as LocationState) ?? {};

  // Step 1 — Source
  const [locations, setLocations] = useState<CommonLocation[]>([]);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [sourcePath, setSourcePath] = useState(locState.device?.path ?? "");
  const [sourceLabel, setSourceLabel] = useState(
    locState.device ? locState.device.name : ""
  );
  const [sourceDevice, setSourceDevice] = useState<DeviceInfo | null>(locState.device ?? null);
  const [loadingLocations, setLoadingLocations] = useState(true);

  // Step 2 — Mode
  const [mode, setMode] = useState<string>(locState.mode ?? "quick");

  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load common locations + devices on mount
  useEffect(() => {
    Promise.all([
      api.getCommonLocations().catch(() => [] as CommonLocation[]),
      api.listDevices().catch(() => ({ devices: [] as DeviceInfo[], error: null })),
    ]).then(([locs, devRes]) => {
      setLocations(locs);
      setDevices(devRes.devices ?? []);
      setLoadingLocations(false);
    });
  }, []);

  const selectedMode = MODES.find((m) => m.id === mode) ?? MODES[0];

  const handleSelectLocation = (loc: CommonLocation) => {
    setSourcePath(loc.path);
    setSourceLabel(loc.label);
    setSourceDevice(null);
  };

  const handleSelectDevice = (dev: DeviceInfo) => {
    setSourcePath(dev.path);
    setSourceLabel(dev.name);
    setSourceDevice(dev);
  };

  const handleStart = async () => {
    if (!sourcePath.trim()) {
      setError("Please select a source location or enter a path.");
      return;
    }
    setCreating(true);
    setError(null);

    try {
      const req: CreateSessionRequest = {
        source_path: sourcePath.trim(),
        source_size_bytes: sourceDevice?.size_bytes ?? 0,
        sector_size: sourceDevice?.sector_size ?? 512,
        scan_mode: mode,
        ...selectedMode.config,
        deleted_after: null,
        deleted_before: null,
        filter_prefix: null,
      };

      const session = await api.createSession(req);
      navigate("/scanning", { state: { session } });
    } catch (err) {
      setError(String(err));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "24px" }}>
        <button className="btn btn-ghost" onClick={() => navigate("/devices")}>← Devices</button>
        <div>
          <h2>Scan Setup</h2>
          <p style={{ color: "var(--text-secondary)", fontSize: "0.8125rem", marginTop: "2px" }}>
            Select a source and scan mode, then start scanning.
          </p>
        </div>
      </div>

      {error && <div className="alert alert-error" style={{ marginBottom: "16px" }}>{error}</div>}

      {/* ── STEP 1: Source ── */}
      <div className="card" style={{ marginBottom: "16px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "16px" }}>
          <h3>① Source</h3>
          {sourcePath && (
            <span style={{ fontSize: "0.8125rem", color: "var(--accent-green)" }}>
              ✓ {sourceLabel || sourcePath}
            </span>
          )}
        </div>

        {/* Common locations */}
        {!loadingLocations && (
          <>
            <p style={{ fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: "10px", textTransform: "uppercase", letterSpacing: "0.06em" }}>
              Common Locations
            </p>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "8px", marginBottom: "16px" }}>
              {locations.filter(l => l.exists).map((loc) => (
                <button
                  key={loc.path}
                  onClick={() => handleSelectLocation(loc)}
                  style={{
                    padding: "12px 10px",
                    background: sourcePath === loc.path ? "var(--accent-blue)" : "var(--bg-primary)",
                    border: `1px solid ${sourcePath === loc.path ? "var(--accent-blue)" : "var(--border)"}`,
                    borderRadius: "8px",
                    color: sourcePath === loc.path ? "#fff" : "var(--text-secondary)",
                    cursor: "pointer",
                    textAlign: "center",
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "center",
                    gap: "6px",
                  }}
                >
                  <span style={{ fontSize: "1.5rem" }}>{loc.icon}</span>
                  <span style={{ fontSize: "0.8125rem", fontWeight: 600 }}>{loc.label}</span>
                  {loc.is_trash && (
                    <span style={{ fontSize: "0.65rem", color: sourcePath === loc.path ? "#ddd6fe" : "var(--accent-amber)" }}>
                      Trash
                    </span>
                  )}
                </button>
              ))}
            </div>
          </>
        )}

        {/* Physical devices */}
        {devices.filter(d => d.device_type !== "virtual").length > 0 && (
          <>
            <p style={{ fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: "10px", textTransform: "uppercase", letterSpacing: "0.06em" }}>
              Storage Devices
            </p>
            <div style={{ display: "flex", flexDirection: "column", gap: "6px", marginBottom: "16px" }}>
              {devices.filter(d => d.device_type !== "virtual").map((dev) => (
                <button
                  key={dev.path}
                  onClick={() => dev.is_accessible && handleSelectDevice(dev)}
                  style={{
                    padding: "10px 14px",
                    background: sourcePath === dev.path ? "var(--accent-blue)" : "var(--bg-primary)",
                    border: `1px solid ${sourcePath === dev.path ? "var(--accent-blue)" : "var(--border)"}`,
                    borderRadius: "8px",
                    color: sourcePath === dev.path ? "#fff" : dev.is_accessible ? "var(--text-primary)" : "var(--text-muted)",
                    cursor: dev.is_accessible ? "pointer" : "not-allowed",
                    display: "flex",
                    alignItems: "center",
                    gap: "12px",
                    opacity: dev.is_accessible ? 1 : 0.5,
                    textAlign: "left",
                  }}
                >
                  <span style={{ fontSize: "1.25rem" }}>{dev.is_internal ? "🖥️" : "🔌"}</span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 600, fontSize: "0.875rem" }}>{dev.name}</div>
                    <div style={{ fontSize: "0.75rem", opacity: 0.8 }}>
                      {dev.path} · {formatBytes(dev.size_bytes)}
                      {!dev.is_accessible && " · No Access (grant Full Disk Access)"}
                    </div>
                  </div>
                </button>
              ))}
            </div>
          </>
        )}

        {/* Manual path */}
        <div>
          <p style={{ fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: "6px", textTransform: "uppercase", letterSpacing: "0.06em" }}>
            Or enter a path manually
          </p>
          <input
            className="form-input mono"
            type="text"
            value={sourcePath}
            onChange={(e) => { setSourcePath(e.target.value); setSourceLabel(""); setSourceDevice(null); }}
            placeholder="/dev/disk2  or  /path/to/image.img  or  /Users/you/Downloads"
          />
        </div>
      </div>

      {/* ── STEP 2: Scan Mode ── */}
      <div className="card" style={{ marginBottom: "24px" }}>
        <h3 style={{ marginBottom: "16px" }}>② Scan Mode</h3>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "10px", marginBottom: "12px" }}>
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
                textAlign: "left",
                display: "flex",
                alignItems: "center",
                gap: "8px",
              }}
            >
              <span style={{ fontSize: "1.2rem" }}>{m.icon}</span>
              {m.label}
            </button>
          ))}
        </div>
        <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", lineHeight: 1.6 }}>
          {selectedMode.description}
        </p>
      </div>

      {/* Actions */}
      <div style={{ display: "flex", gap: "12px", alignItems: "center" }}>
        <button
          className="btn btn-primary btn-lg"
          onClick={handleStart}
          disabled={creating || !sourcePath.trim()}
        >
          {creating ? "Starting…" : `▶ Start ${selectedMode.label}`}
        </button>
        <button className="btn btn-ghost btn-lg" onClick={() => navigate(-1)}>
          Cancel
        </button>
        {sourcePath && (
          <span style={{ fontSize: "0.8125rem", color: "var(--text-muted)", marginLeft: "auto" }}>
            {sourceLabel || sourcePath} · {selectedMode.label}
          </span>
        )}
      </div>

      <div className="alert alert-info" style={{ marginTop: "20px" }}>
        <strong>Safety:</strong> The source is always opened read-only. RecoverX never modifies the selected location.
      </div>
    </div>
  );
}
