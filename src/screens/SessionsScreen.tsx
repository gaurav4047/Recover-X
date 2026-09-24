import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, formatBytes, sessionStatusColor } from "../lib/api";
import type { ScanSession } from "../lib/api";

export function SessionsScreen() {
  const navigate = useNavigate();
  const [sessions, setSessions] = useState<ScanSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // single-session delete
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  // delete-all
  const [confirmDeleteAll, setConfirmDeleteAll] = useState(false);
  const [deletingAll, setDeletingAll] = useState(false);
  const [deleteAllProgress, setDeleteAllProgress] = useState<{ done: number; total: number } | null>(null);

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

  // Only show sessions that were actually started — filter out ones that were
  // created but abandoned before a scan began (status === "created", no files, no bytes).
  const visibleSessions = sessions.filter(
    s => !(s.status === "created" && s.processed_bytes === 0 && s.files_found === 0)
  );

  // ── single delete ──────────────────────────────────────────────────────────
  const handleDeleteConfirm = async () => {
    if (!confirmDeleteId) return;
    setDeleting(true);
    try {
      await api.deleteSession(confirmDeleteId);
      setConfirmDeleteId(null);
      load();
    } catch (err) {
      setError(String(err));
      setConfirmDeleteId(null);
    } finally {
      setDeleting(false);
    }
  };

  // ── delete all ─────────────────────────────────────────────────────────────
  const handleDeleteAll = async () => {
    setDeletingAll(true);
    setDeleteAllProgress({ done: 0, total: visibleSessions.length });
    let failed = 0;
    for (let i = 0; i < visibleSessions.length; i++) {
      try {
        await api.deleteSession(visibleSessions[i].id);
      } catch {
        failed++;
      }
      setDeleteAllProgress({ done: i + 1, total: visibleSessions.length });
    }
    setDeletingAll(false);
    setConfirmDeleteAll(false);
    setDeleteAllProgress(null);
    if (failed > 0) setError(`${failed} session${failed !== 1 ? "s" : ""} could not be deleted.`);
    load();
  };

  const anyBusy = deleting || deletingAll;

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>

      {/* ── Header ── */}
      <div style={{
        display: "flex", alignItems: "flex-start",
        justifyContent: "space-between", marginBottom: "24px", gap: 12,
      }}>
        <div>
          <h2 style={{ margin: 0 }}>Scan Sessions</h2>
          <p style={{ color: "var(--text-secondary)", marginTop: "4px", fontSize: "0.875rem" }}>
            Past and active file recovery scans. Corruption repair jobs are not stored here.
          </p>
        </div>

        <div style={{ display: "flex", gap: 8, alignItems: "center", flexShrink: 0 }}>
          <button className="btn btn-secondary" onClick={load} disabled={loading || anyBusy}>
            {loading ? "Loading…" : "↻ Refresh"}
          </button>

          {visibleSessions.length > 0 && (
            <button
              className="btn btn-danger"
              onClick={() => { setConfirmDeleteAll(true); setConfirmDeleteId(null); }}
              disabled={anyBusy}
            >
              🗑 Delete All
            </button>
          )}
        </div>
      </div>

      {/* ── Delete-All confirmation banner ── */}
      {confirmDeleteAll && (
        <div style={{
          display: "flex", alignItems: "center", gap: 12,
          padding: "12px 16px", marginBottom: 20,
          background: "var(--bg-secondary)",
          border: "1px solid var(--accent-red, #ef4444)",
          borderRadius: 8,
          flexWrap: "wrap",
        }}>
          {deletingAll ? (
            <span style={{ fontSize: "0.875rem", color: "var(--text-primary)", flex: 1 }}>
              ⏳ Deleting {deleteAllProgress?.done} / {deleteAllProgress?.total} sessions…
            </span>
          ) : (
            <>
              <span style={{ fontSize: "0.875rem", color: "var(--accent-red, #ef4444)", flex: 1 }}>
                ⚠️ Delete all <strong>{visibleSessions.length}</strong> session{visibleSessions.length !== 1 ? "s" : ""} and their data? This cannot be undone.
              </span>
              <button
                className="btn btn-danger"
                style={{ padding: "6px 16px", fontSize: "0.8125rem" }}
                onClick={handleDeleteAll}
              >
                Yes, Delete All
              </button>
              <button
                className="btn btn-ghost"
                style={{ padding: "6px 16px", fontSize: "0.8125rem" }}
                onClick={() => setConfirmDeleteAll(false)}
              >
                Cancel
              </button>
            </>
          )}
        </div>
      )}

      {error && <div className="alert alert-error" style={{ marginBottom: 16 }}>{error}</div>}

      {/* ── Empty state ── */}
      {!loading && visibleSessions.length === 0 && (
        <div className="empty-state">
          <div className="empty-icon">📋</div>
          <h3>No sessions found</h3>
          <p>Start a scan from the Devices screen.</p>
          <button
            className="btn btn-primary"
            style={{ marginTop: "16px" }}
            onClick={() => navigate("/devices")}
          >
            Go to Devices
          </button>
        </div>
      )}

      {/* ── Session list ── */}
      <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
        {visibleSessions.map((s) => (
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

              {/* Per-session inline confirmation */}
              {confirmDeleteId === s.id && (
                <div style={{
                  display: "flex", alignItems: "center", gap: "10px",
                  marginTop: "10px", padding: "8px 12px",
                  background: "var(--bg-primary)", borderRadius: "6px",
                  border: "1px solid var(--accent-red)",
                }}>
                  <span style={{ fontSize: "0.8125rem", color: "var(--accent-red)" }}>
                    Delete this session and all its data?
                  </span>
                  <button
                    className="btn btn-danger"
                    style={{ padding: "4px 12px", fontSize: "0.8125rem" }}
                    onClick={handleDeleteConfirm}
                    disabled={deleting}
                  >
                    {deleting ? "Deleting…" : "Yes, Delete"}
                  </button>
                  <button
                    className="btn btn-ghost"
                    style={{ padding: "4px 12px", fontSize: "0.8125rem" }}
                    onClick={() => setConfirmDeleteId(null)}
                    disabled={deleting}
                  >
                    Cancel
                  </button>
                </div>
              )}
            </div>

            <div style={{ display: "flex", gap: "8px", flexShrink: 0 }}>
              <button
                className="btn btn-secondary"
                onClick={() => {
                  if (s.status === "running" || s.status === "paused") {
                    navigate("/scanning", { state: { session: s } });
                  } else {
                    navigate("/results", { state: { session: s, sessionId: s.id } });
                  }
                }}
              >
                View
              </button>
              {s.status === "paused" && (
                <button className="btn btn-primary" onClick={() =>
                  navigate("/scanning", { state: { session: s } })
                }>
                  Resume
                </button>
              )}
              <button
                className="btn btn-danger"
                onClick={() => { setConfirmDeleteId(s.id); setConfirmDeleteAll(false); }}
                disabled={anyBusy}
              >
                🗑 Delete
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
