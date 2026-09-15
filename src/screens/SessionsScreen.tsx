import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, formatBytes, sessionStatusColor } from "../lib/api";
import type { ScanSession } from "../lib/api";

export function SessionsScreen() {
  const navigate = useNavigate();
  const [sessions, setSessions] = useState<ScanSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      setSessions(await api.listSessions());
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { load(); }, []);

  const handleDelete = async (id: string) => {
    if (!confirm("Delete this session?")) return;
    try {
      await api.deleteSession(id);
      load();
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "24px" }}>
        <div>
          <h2>Scan Sessions</h2>
          <p style={{ color: "var(--text-secondary)", marginTop: "4px", fontSize: "0.875rem" }}>
            All past and current scan sessions.
          </p>
        </div>
        <button className="btn btn-secondary" onClick={load} disabled={loading}>
          {loading ? "Loading…" : "↻ Refresh"}
        </button>
      </div>

      {error && <div className="alert alert-error">{error}</div>}

      {!loading && sessions.length === 0 && (
        <div className="empty-state">
          <div className="empty-icon">📋</div>
          <h3>No sessions found</h3>
          <p>Start a scan from the Devices screen.</p>
          <button className="btn btn-primary" style={{ marginTop: "16px" }} onClick={() => navigate("/devices")}>
            Go to Devices
          </button>
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
        {sessions.map((s) => (
          <div key={s.id} className="card" style={{ display: "flex", alignItems: "center", gap: "16px" }}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: "flex", alignItems: "center", gap: "10px", marginBottom: "4px" }}>
                <span className="mono" style={{ fontSize: "0.8125rem" }}>{s.id.slice(0, 16)}…</span>
                <span style={{
                  fontSize: "0.75rem", padding: "2px 8px", borderRadius: "12px",
                  background: sessionStatusColor(s.status) + "33",
                  color: sessionStatusColor(s.status),
                  fontWeight: 600,
                }}>
                  {s.status}
                </span>
              </div>
              <div style={{ display: "flex", gap: "16px", fontSize: "0.8125rem", color: "var(--text-secondary)" }}>
                <span className="mono">{s.source_identity.path}</span>
                <span>{formatBytes(s.source_size_bytes)}</span>
                <span>{s.configuration.mode}</span>
                <span>{s.files_found} files</span>
              </div>
            </div>
            <div style={{ display: "flex", gap: "8px", flexShrink: 0 }}>
              {s.status === "paused" && (
                <button className="btn btn-primary" onClick={() =>
                  navigate("/scanning", { state: { session: s } })
                }>
                  Resume
                </button>
              )}
              <button className="btn btn-ghost" onClick={() => handleDelete(s.id)}>
                Delete
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
