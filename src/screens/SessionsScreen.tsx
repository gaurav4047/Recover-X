import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, formatBytes, sessionStatusColor } from "../lib/api";
import type { ScanSession } from "../lib/api";

export function SessionsScreen() {
  const navigate = useNavigate();
  const [sessions, setSessions] = useState<ScanSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [confirmId, setConfirmId] = useState<string | null>(null);

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
    setDeletingId(id);
    setConfirmId(null);
    try {
      await api.deleteSession(id);
      setSessions((prev) => prev.filter((s) => s.id !== id));
    } catch (err) {
      setError(String(err));
    } finally {
      setDeletingId(null);
    }
  };

  const handleDeleteAll = async () => {
    const ids = sessions.map((s) => s.id);
    for (const id of ids) {
      try { await api.deleteSession(id); } catch (_) {}
    }
    setSessions([]);
    setConfirmId(null);
  };

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "24px" }}>
        <div>
          <h2>Scan Sessions</h2>
          <p style={{ color: "var(--text-secondary)", marginTop: "4px", fontSize: "0.875rem" }}>
            {sessions.length} session{sessions.length !== 1 ? "s" : ""}
          </p>
        </div>
        <div style={{ display: "flex", gap: "8px" }}>
          {sessions.length > 0 && (
            confirmId === "all" ? (
              <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
                <span style={{ fontSize: "0.8125rem", color: "var(--text-secondary)" }}>Delete all?</span>
                <button className="btn btn-danger" onClick={handleDeleteAll}>Yes, delete all</button>
                <button className="btn btn-ghost" onClick={() => setConfirmId(null)}>Cancel</button>
              </div>
            ) : (
              <button className="btn btn-secondary" onClick={() => setConfirmId("all")}>
                🗑 Delete All
              </button>
            )
          )}
          <button className="btn btn-secondary" onClick={load} disabled={loading}>
            {loading ? "Loading…" : "↻ Refresh"}
          </button>
        </div>
      </div>

      {error && <div className="alert alert-error" style={{ marginBottom: "12px" }}>{error}</div>}

      {!loading && sessions.length === 0 && (
        <div className="empty-state">
          <div className="empty-icon">🗂️</div>
          <h3>No sessions found</h3>
          <p>Start a scan to see sessions here.</p>
          <button className="btn btn-primary" style={{ marginTop: "16px" }} onClick={() => navigate("/scan-setup")}>
            Start a Scan
          </button>
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
        {sessions.map((s) => (
          <div key={s.id} className="card" style={{ padding: "16px" }}>
            <div style={{ display: "flex", alignItems: "flex-start", gap: "14px" }}>
              {/* Info */}
              <div style={{ flex: 1, minWidth: 0 }}>
                {/* Status + date */}
                <div style={{ display: "flex", alignItems: "center", gap: "10px", marginBottom: "6px", flexWrap: "wrap" }}>
                  <span style={{
                    fontSize: "0.72rem",
                    padding: "2px 9px",
                    borderRadius: "12px",
                    background: sessionStatusColor(s.status) + "33",
                    color: sessionStatusColor(s.status),
                    fontWeight: 700,
                    textTransform: "uppercase",
                    letterSpacing: "0.04em",
                  }}>
                    {s.status}
                  </span>
                  <span style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
                    {new Date(s.created_at).toLocaleString()}
                  </span>
                </div>

                {/* Source path */}
                <div className="mono" style={{ fontSize: "0.8125rem", marginBottom: "4px", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {s.source_identity.path}
                </div>

                {/* Stats */}
                <div style={{ display: "flex", gap: "14px", fontSize: "0.8125rem", color: "var(--text-secondary)", flexWrap: "wrap" }}>
                  <span>📁 {Number(s.files_found).toLocaleString()} files found</span>
                  {s.source_size_bytes > 0 && <span>💾 {formatBytes(s.source_size_bytes)}</span>}
                  <span style={{ textTransform: "capitalize" }}>
                    {s.configuration.mode.replace("_", " ")} scan
                  </span>
                  {s.bad_sectors > 0 && (
                    <span style={{ color: "var(--accent-red)" }}>⚠️ {Number(s.bad_sectors)} bad sectors</span>
                  )}
                </div>

                {s.last_error && (
                  <div style={{ marginTop: "6px", fontSize: "0.75rem", color: "var(--accent-red)" }}>
                    Error: {s.last_error}
                  </div>
                )}
              </div>

              {/* Action buttons */}
              <div style={{ display: "flex", gap: "6px", flexShrink: 0, alignItems: "flex-start" }}>
                {/* View Results — only when completed with files */}
                {s.status === "completed" && Number(s.files_found) > 0 && (
                  <button
                    className="btn btn-primary"
                    onClick={() => navigate("/results", { state: { sessionId: s.id } })}
                  >
                    View Results
                  </button>
                )}

                {/* Resume — paused sessions */}
                {s.status === "paused" && (
                  <button
                    className="btn btn-secondary"
                    onClick={() => navigate("/scanning", { state: { session: s } })}
                  >
                    ▶ Resume
                  </button>
                )}

                {/* Re-scan — completed or failed */}
                {(s.status === "completed" || s.status === "failed") && (
                  <button
                    className="btn btn-secondary"
                    onClick={() => navigate("/scanning", { state: { session: s } })}
                  >
                    ↺ Re-scan
                  </button>
                )}

                {/* Delete with inline confirm */}
                {confirmId === s.id ? (
                  <>
                    <button
                      className="btn btn-danger"
                      disabled={deletingId === s.id}
                      onClick={() => handleDelete(s.id)}
                    >
                      {deletingId === s.id ? "Deleting…" : "Confirm"}
                    </button>
                    <button className="btn btn-ghost" onClick={() => setConfirmId(null)}>
                      Cancel
                    </button>
                  </>
                ) : (
                  <button
                    className="btn btn-ghost"
                    style={{ color: "var(--accent-red)" }}
                    onClick={() => setConfirmId(s.id)}
                  >
                    🗑 Delete
                  </button>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
