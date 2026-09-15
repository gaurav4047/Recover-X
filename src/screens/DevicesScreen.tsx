import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, formatBytes } from "../lib/api";
import type { DeviceInfo, DeviceListResponse } from "../lib/api";

export function DevicesScreen() {
  const navigate = useNavigate();
  const [response, setResponse] = useState<DeviceListResponse | null>(null);
  const [loading, setLoading] = useState(true);

  const loadDevices = async () => {
    setLoading(true);
    try {
      const res = await api.listDevices();
      setResponse(res);
    } catch (err) {
      setResponse({ devices: [], error: String(err) });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadDevices();
  }, []);

  const internal = response?.devices.filter((d) => d.is_internal) ?? [];
  const external = response?.devices.filter((d) => !d.is_internal && d.device_type !== "optical") ?? [];

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "24px" }}>
        <div>
          <h2>Storage Devices</h2>
          <p style={{ color: "var(--text-secondary)", marginTop: "4px", fontSize: "0.875rem" }}>
            Detected internal and external storage on this system.
          </p>
        </div>
        <button className="btn btn-secondary" onClick={loadDevices} disabled={loading}>
          {loading ? "Detecting…" : "↻ Refresh"}
        </button>
      </div>

      {!response?.error && (
        <div className="alert alert-info" style={{ marginBottom: "20px" }}>
          Raw device access requires <strong>Full Disk Access</strong> permission.
          {" "}<strong>To grant it:</strong> run this in Terminal →{" "}
          <code style={{ userSelect: "all" }}>
            open "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
          </code>
          {" "}then add{" "}
          <code style={{ userSelect: "all" }}>/Users/gauravgavali/recoverx/target/debug/recoverx</code>
          {" "}and toggle it ON. Then restart with <code>sudo npm run tauri dev</code>.
        </div>
      )}

      {loading && (
        <div className="empty-state">
          <div className="empty-icon">⏳</div>
          <h3>Detecting devices…</h3>
        </div>
      )}

      {!loading && response?.error && (
        <div className="alert alert-error">
          <strong>Device enumeration failed:</strong> {response.error}
        </div>
      )}

      {!loading && !response?.error && response?.devices.length === 0 && (
        <div className="empty-state">
          <div className="empty-icon">💾</div>
          <h3>No devices detected</h3>
          <p>Run with elevated privileges to detect physical storage devices.</p>
          <p style={{ marginTop: "8px" }}>On macOS: <code>sudo ./recoverx</code></p>
        </div>
      )}

      {!loading && response && response.devices.length > 0 && (
        <>
          {internal.length > 0 && (
            <section style={{ marginBottom: "28px" }}>
              <h3 style={{ marginBottom: "12px", color: "var(--text-secondary)", fontSize: "0.75rem", textTransform: "uppercase", letterSpacing: "0.08em" }}>
                INTERNAL
              </h3>
              <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
                {internal.map((device) => (
                  <DeviceCard key={device.path} device={device} onScan={(mode) =>
                    navigate("/scan-setup", { state: { device, mode } })
                  } />
                ))}
              </div>
            </section>
          )}

          {external.length > 0 && (
            <section>
              <h3 style={{ marginBottom: "12px", color: "var(--text-secondary)", fontSize: "0.75rem", textTransform: "uppercase", letterSpacing: "0.08em" }}>
                EXTERNAL / REMOVABLE
              </h3>
              <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
                {external.map((device) => (
                  <DeviceCard key={device.path} device={device} onScan={(mode) =>
                    navigate("/scan-setup", { state: { device, mode } })
                  } />
                ))}
              </div>
            </section>
          )}
        </>
      )}
    </div>
  );
}

interface DeviceCardProps {
  device: DeviceInfo;
  onScan: (mode: "quick" | "deep" | "file_carving") => void;
}

function DeviceCard({ device, onScan }: DeviceCardProps) {
  const typeLabel = device.is_internal ? "INTERNAL" :
    device.device_type === "sd_card" ? "SD CARD" :
    device.device_type === "removable" ? "REMOVABLE" : "EXTERNAL";

  const typeColor = device.is_internal ? "var(--accent-blue)" :
    device.device_type === "sd_card" ? "var(--accent-green)" : "var(--accent-amber)";

  return (
    <div className="card" style={{ display: "flex", alignItems: "center", gap: "16px" }}>
      <div style={{ fontSize: "2rem", flexShrink: 0 }}>
        {device.is_internal ? "🖥️" :
         device.device_type === "sd_card" ? "💳" :
         device.device_type === "removable" ? "🔌" : "💾"}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: "10px", marginBottom: "6px", flexWrap: "wrap" }}>
          <span style={{
            fontSize: "0.65rem",
            padding: "2px 6px",
            borderRadius: "4px",
            background: typeColor + "22",
            color: typeColor,
            fontWeight: 700,
            letterSpacing: "0.05em",
          }}>
            {typeLabel}
          </span>
          <h4 style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {device.name}
          </h4>
          {device.is_write_protected && (
            <span className="badge badge-amber" style={{ fontSize: "0.7rem" }}>Read Only</span>
          )}
          {!device.is_accessible && (
            <span className="badge badge-red" style={{ fontSize: "0.7rem" }}>No Access</span>
          )}
        </div>
        <div style={{ display: "flex", gap: "16px", fontSize: "0.8125rem", color: "var(--text-secondary)", flexWrap: "wrap" }}>
          <span className="mono">{device.path}</span>
          <span>{formatBytes(device.size_bytes)}</span>
          <span>Sector: {device.sector_size}B</span>
          {device.filesystem && <span>FS: {device.filesystem}</span>}
          {device.model && <span>{device.model}</span>}
        </div>
      </div>
      <div style={{ display: "flex", gap: "8px", flexShrink: 0 }}>
        <button
          className="btn btn-secondary"
          onClick={() => onScan("quick")}
          disabled={!device.is_accessible}
          title="Fast scan: filesystem metadata and deleted entries only"
        >
          Quick Scan
        </button>
        <button
          className="btn btn-primary"
          onClick={() => onScan("deep")}
          disabled={!device.is_accessible}
          title="Full scan: filesystem + deleted entries + file carving"
        >
          Deep Scan
        </button>
      </div>
    </div>
  );
}
