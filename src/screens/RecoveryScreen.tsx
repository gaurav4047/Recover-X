import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, formatBytes } from "../lib/api";
import type { RecoveryResult, ScanSession } from "../lib/api";

interface LocationState {
  sessionId?: string;
  fileIds?: string[];
  sourcePath?: string;
}

export function RecoveryScreen() {
  const location = useLocation();
  const navigate = useNavigate();
  const locState = (location.state as LocationState) ?? {};

  const [sessionId] = useState<string | null>(locState.sessionId ?? null);
  const [fileIds] = useState<string[]>(locState.fileIds ?? []);
  const [destination, setDestination] = useState("");
  const [recovering, setRecovering] = useState(false);
  const [results, setResults] = useState<RecoveryResult[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sourcePath, setSourcePath] = useState("");

  // Load session to get source path
  useEffect(() => {
    if (!sessionId) return;
    api.loadSession(sessionId).then((session: ScanSession) => {
      setSourcePath(session.source_identity.path);
    }).catch(() => {});
  }, [sessionId]);

  const handleRecover = async () => {
    if (!sessionId) { setError("No scan session selected."); return; }
    if (!destination.trim()) { setError("Please specify a destination folder."); return; }
    if (!sourcePath) { setError("Source path is not set."); return; }

    // Safety: warn if destination looks like a device path
    if (destination.startsWith("/dev/") || destination.startsWith("\\\\.\\")) {
      setError("Destination cannot be a raw device path. Choose a folder on a separate volume.");
      return;
    }

    setRecovering(true);
    setError(null);

    try {
      const res = await api.recoverFiles({
        session_id: sessionId,
        source_path: sourcePath,
        file_ids: fileIds,
        destination_dir: destination.trim(),
      });
      setResults(res);
    } catch (e) {
      setError(String(e));
    } finally {
      setRecovering(false);
    }
  };

  const successCount = results?.filter((r) => r.status === "success").length ?? 0;
  const failCount = results?.filter((r) => r.status === "failed").length ?? 0;
  const totalBytes = results?.reduce((s, r) => s + r.size_recovered, 0) ?? 0;

  if (!sessionId) {
    return (
      <div style={{ padding: "24px" }}>
        <div className="empty-state">
          <div className="empty-icon">↗️</div>
          <h3>No files selected for recovery</h3>
          <p>Go to Results and select files to recover.</p>
          <button className="btn btn-primary" style={{ marginTop: "16px" }} onClick={() => navigate("/results")}>
            Go to Results
          </button>
        </div>
      </div>
    );
  }

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "24px" }}>
        <button className="btn btn-ghost" onClick={() => navigate(-1)}>← Results</button>
        <h2>Recovery</h2>
      </div>

      {error && <div className="alert alert-error" style={{ marginBottom: "16px" }}>{error}</div>}

      {/* Summary */}
      {!results && (
        <div className="card" style={{ marginBottom: "16px" }}>
          <h3 style={{ marginBottom: "12px" }}>Recovery Summary</h3>
          <div style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "8px" }}>
            {fileIds.length > 0
              ? `${fileIds.length} file${fileIds.length !== 1 ? "s" : ""} selected`
              : "All recoverable files"}
          </div>
          <div style={{ fontSize: "0.875rem", color: "var(--text-muted)" }}>
            Source: <span className="mono">{sourcePath}</span>
          </div>
        </div>
      )}

      {/* Destination */}
      {!results && (
        <div className="card" style={{ marginBottom: "16px" }}>
          <h3 style={{ marginBottom: "16px" }}>Destination Folder</h3>
          <div className="form-group" style={{ marginBottom: "8px" }}>
            <label className="form-label">Destination path</label>
            <input
              className="form-input mono"
              type="text"
              value={destination}
              onChange={(e) => setDestination(e.target.value)}
              placeholder="/Users/you/Desktop/RecoverX-Output"
              disabled={recovering}
            />
          </div>
          <div className="alert alert-warning">
            <strong>Important:</strong> The destination must be on a <em>different drive</em> from the source.
            Writing to the same drive may overwrite the data you are trying to recover.
          </div>
        </div>
      )}

      {/* Source protection notice */}
      {!results && (
        <div className="alert alert-info" style={{ marginBottom: "16px" }}>
          Source <code>{sourcePath}</code> will be opened read-only.
          RecoverX will not modify the source device.
        </div>
      )}

      {/* Action */}
      {!results && (
        <button
          className="btn btn-primary btn-lg"
          onClick={handleRecover}
          disabled={recovering || !destination.trim()}
        >
          {recovering ? "Recovering…" : `↗ Recover ${fileIds.length > 0 ? fileIds.length : "All"} Files`}
        </button>
      )}

      {/* Progress */}
      {recovering && (
        <div style={{ marginTop: "20px" }}>
          <div className="progress-bar-track">
            <div className="progress-bar-fill" style={{ width: "100%", animation: "pulse 1.5s infinite" }} />
          </div>
          <p style={{ color: "var(--text-secondary)", marginTop: "8px", fontSize: "0.875rem" }}>
            Writing files to destination with SHA-256 verification…
          </p>
        </div>
      )}

      {/* Results */}
      {results && (
        <div>
          {/* Summary card */}
          <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "12px", marginBottom: "20px" }}>
            <div className="card" style={{ textAlign: "center", borderColor: "var(--accent-green)" }}>
              <div style={{ fontSize: "2rem", fontWeight: 700, color: "var(--accent-green)" }}>{successCount}</div>
              <div style={{ fontSize: "0.75rem", color: "var(--text-muted)", marginTop: "4px" }}>RECOVERED</div>
            </div>
            <div className="card" style={{ textAlign: "center" }}>
              <div style={{ fontSize: "2rem", fontWeight: 700 }}>{formatBytes(totalBytes)}</div>
              <div style={{ fontSize: "0.75rem", color: "var(--text-muted)", marginTop: "4px" }}>TOTAL SIZE</div>
            </div>
            <div className="card" style={{ textAlign: "center", borderColor: failCount > 0 ? "var(--accent-red)" : "var(--border)" }}>
              <div style={{ fontSize: "2rem", fontWeight: 700, color: failCount > 0 ? "var(--accent-red)" : "var(--text-primary)" }}>
                {failCount}
              </div>
              <div style={{ fontSize: "0.75rem", color: "var(--text-muted)", marginTop: "4px" }}>FAILED</div>
            </div>
          </div>

          {/* File result list */}
          <div className="card">
            <h3 style={{ marginBottom: "14px" }}>Recovery Log</h3>
            <div style={{ maxHeight: "400px", overflow: "auto" }}>
              {results.map((r) => (
                <div key={r.file_id} style={{
                  display: "flex",
                  alignItems: "flex-start",
                  gap: "12px",
                  padding: "10px 0",
                  borderBottom: "1px solid var(--border)",
                  fontSize: "0.8125rem",
                }}>
                  <span style={{ fontSize: "1.1rem", flexShrink: 0 }}>
                    {r.status === "success" ? "✅" : r.status === "partial" ? "⚠️" : "❌"}
                  </span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {r.name}
                    </div>
                    {r.status === "success" && (
                      <>
                        <div style={{ color: "var(--text-muted)", fontSize: "0.75rem", marginTop: "2px" }}>
                          → {r.destination_path}
                        </div>
                        {r.sha256 && (
                          <div style={{ color: "var(--text-muted)", fontSize: "0.7rem", marginTop: "2px" }} className="mono">
                            SHA-256: {r.sha256}
                          </div>
                        )}
                      </>
                    )}
                    {r.error && (
                      <div style={{ color: "var(--accent-red)", fontSize: "0.75rem", marginTop: "2px" }}>
                        {r.error}
                      </div>
                    )}
                  </div>
                  <div style={{ color: "var(--text-muted)", whiteSpace: "nowrap" }}>
                    {formatBytes(r.size_recovered)}
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Post-recovery actions */}
          <div style={{ display: "flex", gap: "12px", marginTop: "20px" }}>
            <button className="btn btn-secondary" onClick={() => navigate("/results", { state: { sessionId } })}>
              ← Back to Results
            </button>
            <button className="btn btn-ghost" onClick={() => navigate("/")}>
              Home
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
