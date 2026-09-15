import { useEffect, useState, useMemo } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, formatBytes, confidenceColor, fileStatusLabel, fileIcon } from "../lib/api";
import type { RecoveredFile, ScanSession } from "../lib/api";

interface LocationState {
  sessionId?: string;
  session?: ScanSession;
}

type SortKey = "name" | "size" | "confidence" | "modified";
type FilterStatus = "all" | "complete" | "partial" | "fragmented" | "corrupted";

export function ResultsScreen() {
  const location = useLocation();
  const navigate = useNavigate();
  const locState = (location.state as LocationState) ?? {};

  const [sessionId, setSessionId] = useState<string | null>(
    locState.sessionId ?? locState.session?.id ?? null
  );
  const [files, setFiles] = useState<RecoveredFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [search, setSearch] = useState("");
  const [filterExt, setFilterExt] = useState("");
  const [filterStatus, setFilterStatus] = useState<FilterStatus>("all");
  const [minConfidence, setMinConfidence] = useState(0);
  const [sortKey, setSortKey] = useState<SortKey>("confidence");
  const [sortAsc, setSortAsc] = useState(false);

  // Load from most recent completed session if none specified
  useEffect(() => {
    if (!sessionId) {
      api.listSessions().then((sessions) => {
        const completed = sessions.find((s) => s.status === "completed");
        if (completed) setSessionId(completed.id);
      }).catch(() => {});
    }
  }, []);

  useEffect(() => {
    if (!sessionId) return;
    setLoading(true);
    setError(null);
    api.listRecoveredFiles(sessionId)
      .then((f) => {
        setFiles(f);
        setLoading(false);
      })
      .catch((e) => {
        setError(String(e));
        setLoading(false);
      });
  }, [sessionId]);

  // All unique extensions for the filter dropdown
  const extensions = useMemo(() => {
    const exts = new Set<string>();
    files.forEach((f) => { if (f.extension) exts.add(f.extension.toLowerCase()); });
    return Array.from(exts).sort();
  }, [files]);

  // Filtered + sorted file list
  const filtered = useMemo(() => {
    let result = files;

    if (search) {
      const q = search.toLowerCase();
      result = result.filter(
        (f) =>
          f.name.toLowerCase().includes(q) ||
          (f.original_path ?? "").toLowerCase().includes(q)
      );
    }
    if (filterExt) {
      result = result.filter((f) => f.extension?.toLowerCase() === filterExt);
    }
    if (filterStatus !== "all") {
      result = result.filter((f) => f.status === filterStatus);
    }
    if (minConfidence > 0) {
      result = result.filter((f) => f.confidence >= minConfidence);
    }

    result = [...result].sort((a, b) => {
      let cmp = 0;
      switch (sortKey) {
        case "name": cmp = a.name.localeCompare(b.name); break;
        case "size": cmp = a.size_bytes - b.size_bytes; break;
        case "confidence": cmp = a.confidence - b.confidence; break;
        case "modified":
          cmp = (a.modified_at ?? "").localeCompare(b.modified_at ?? ""); break;
      }
      return sortAsc ? cmp : -cmp;
    });

    return result;
  }, [files, search, filterExt, filterStatus, minConfidence, sortKey, sortAsc]);

  const toggleSelect = (id: string) => {
    setSelected((prev) => {
      const s = new Set(prev);
      if (s.has(id)) s.delete(id); else s.add(id);
      return s;
    });
  };

  const selectAll = () => setSelected(new Set(filtered.map((f) => f.id)));
  const clearAll = () => setSelected(new Set());

  const handleRecover = () => {
    if (selected.size === 0) return;
    navigate("/recovery", {
      state: {
        sessionId,
        fileIds: Array.from(selected),
        sourcePath: files[0]?.session_id ? undefined : undefined,
      },
    });
  };

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) setSortAsc((a) => !a);
    else { setSortKey(key); setSortAsc(false); }
  };

  if (!sessionId) {
    return (
      <div style={{ padding: "24px", height: "100%", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center" }}>
        <div className="empty-state">
          <div className="empty-icon">📋</div>
          <h3>No scan results yet</h3>
          <p>Run a scan from the Devices screen to find recoverable files.</p>
          <button className="btn btn-primary" style={{ marginTop: "20px" }} onClick={() => navigate("/devices")}>
            Scan a Device
          </button>
        </div>
      </div>
    );
  }

  return (
    <div style={{ padding: "24px", height: "100%", display: "flex", flexDirection: "column", overflow: "hidden" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "16px", flexShrink: 0 }}>
        <div>
          <h2>Recovery Results</h2>
          <p style={{ color: "var(--text-secondary)", marginTop: "4px", fontSize: "0.875rem" }}>
            {loading ? "Loading…" : `${filtered.length.toLocaleString()} of ${files.length.toLocaleString()} files`}
            {selected.size > 0 && ` · ${selected.size} selected`}
          </p>
        </div>
        <div style={{ display: "flex", gap: "8px" }}>
          <button className="btn btn-secondary" onClick={() => navigate("/sessions")}>
            Change Session
          </button>
          <button
            className="btn btn-primary"
            onClick={handleRecover}
            disabled={selected.size === 0}
          >
            ↗ Recover {selected.size > 0 ? `(${selected.size})` : "Selected"}
          </button>
        </div>
      </div>

      {error && <div className="alert alert-error" style={{ marginBottom: "12px" }}>{error}</div>}

      {/* Filters */}
      <div style={{ display: "flex", gap: "8px", marginBottom: "12px", flexShrink: 0, flexWrap: "wrap" }}>
        <input
          className="form-input"
          style={{ width: "220px" }}
          type="text"
          placeholder="Search by filename…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <select
          className="form-select"
          style={{ width: "120px" }}
          value={filterExt}
          onChange={(e) => setFilterExt(e.target.value)}
        >
          <option value="">All types</option>
          {extensions.map((ext) => (
            <option key={ext} value={ext}>{ext.toUpperCase()}</option>
          ))}
        </select>
        <select
          className="form-select"
          style={{ width: "140px" }}
          value={filterStatus}
          onChange={(e) => setFilterStatus(e.target.value as FilterStatus)}
        >
          <option value="all">All status</option>
          <option value="complete">Complete</option>
          <option value="partial">Partial</option>
          <option value="fragmented">Fragmented</option>
          <option value="corrupted">Corrupted</option>
        </select>
        <select
          className="form-select"
          style={{ width: "140px" }}
          value={minConfidence}
          onChange={(e) => setMinConfidence(Number(e.target.value))}
        >
          <option value={0}>Any confidence</option>
          <option value={50}>≥ 50%</option>
          <option value={75}>≥ 75%</option>
          <option value={90}>≥ 90%</option>
        </select>
        <div style={{ display: "flex", gap: "6px", marginLeft: "auto" }}>
          <button className="btn btn-ghost" onClick={selectAll} style={{ fontSize: "0.8125rem" }}>Select All</button>
          <button className="btn btn-ghost" onClick={clearAll} style={{ fontSize: "0.8125rem" }}>Clear</button>
        </div>
      </div>

      {/* Table header */}
      <div style={{
        display: "grid",
        gridTemplateColumns: "32px 2fr 1fr 80px 100px 120px",
        gap: "8px",
        padding: "8px 12px",
        fontSize: "0.75rem",
        color: "var(--text-muted)",
        textTransform: "uppercase",
        letterSpacing: "0.06em",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
      }}>
        <span />
        <button
          style={{ background: "none", border: "none", color: "inherit", cursor: "pointer", textAlign: "left", fontSize: "inherit", letterSpacing: "inherit", textTransform: "inherit" }}
          onClick={() => toggleSort("name")}
        >
          Name {sortKey === "name" ? (sortAsc ? "↑" : "↓") : ""}
        </button>
        <span>Path</span>
        <button
          style={{ background: "none", border: "none", color: "inherit", cursor: "pointer", textAlign: "left", fontSize: "inherit", letterSpacing: "inherit", textTransform: "inherit" }}
          onClick={() => toggleSort("size")}
        >
          Size {sortKey === "size" ? (sortAsc ? "↑" : "↓") : ""}
        </button>
        <button
          style={{ background: "none", border: "none", color: "inherit", cursor: "pointer", textAlign: "left", fontSize: "inherit", letterSpacing: "inherit", textTransform: "inherit" }}
          onClick={() => toggleSort("confidence")}
        >
          Confidence {sortKey === "confidence" ? (sortAsc ? "↑" : "↓") : ""}
        </button>
        <span>Status</span>
      </div>

      {/* File list */}
      {loading ? (
        <div className="empty-state" style={{ flex: 1 }}>
          <div className="empty-icon">⏳</div>
          <h3>Loading results…</h3>
        </div>
      ) : filtered.length === 0 ? (
        <div className="empty-state" style={{ flex: 1 }}>
          <div className="empty-icon">📋</div>
          <h3>{files.length === 0 ? "No recoverable files found" : "No files match your filters"}</h3>
          {files.length === 0 && (
            <p>The scan completed but no recoverable files were detected on this source.</p>
          )}
        </div>
      ) : (
        <div style={{ flex: 1, overflow: "auto" }}>
          {filtered.map((file) => (
            <div
              key={file.id}
              style={{
                display: "grid",
                gridTemplateColumns: "32px 2fr 1fr 80px 100px 120px",
                gap: "8px",
                padding: "8px 12px",
                borderBottom: "1px solid var(--border)",
                background: selected.has(file.id) ? "var(--bg-tertiary)" : "transparent",
                cursor: "pointer",
                alignItems: "center",
                fontSize: "0.8125rem",
              }}
              onClick={() => toggleSelect(file.id)}
            >
              <input
                type="checkbox"
                checked={selected.has(file.id)}
                onChange={() => toggleSelect(file.id)}
                onClick={(e) => e.stopPropagation()}
                style={{ accentColor: "var(--accent-blue)" }}
              />
              <div style={{ display: "flex", alignItems: "center", gap: "8px", overflow: "hidden" }}>
                <span style={{ fontSize: "1rem", flexShrink: 0 }}>{fileIcon(file.extension)}</span>
                <div style={{ overflow: "hidden" }}>
                  <div style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {file.name}
                    {!file.original_path && (
                      <span style={{ marginLeft: "6px", fontSize: "0.7rem", color: "var(--text-muted)" }}>(carved)</span>
                    )}
                  </div>
                  <div style={{ fontSize: "0.7rem", color: "var(--text-muted)" }}>
                    {file.filesystem_type ?? file.recovery_method}
                    {file.is_fragmented && " · fragmented"}
                  </div>
                </div>
              </div>
              <div style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: "var(--text-muted)", fontSize: "0.75rem" }}>
                {file.original_path ?? "—"}
              </div>
              <div>{formatBytes(file.size_bytes)}</div>
              <div style={{ color: confidenceColor(file.confidence) }}>
                {file.confidence}%
              </div>
              <div>
                <span style={{
                  fontSize: "0.7rem",
                  padding: "2px 6px",
                  borderRadius: "4px",
                  background:
                    file.status === "complete" ? "#052e16" :
                    file.status === "partial" ? "#451a03" :
                    file.status === "corrupted" ? "#450a0a" : "#1e293b",
                  color:
                    file.status === "complete" ? "#bbf7d0" :
                    file.status === "partial" ? "#fde68a" :
                    file.status === "corrupted" ? "#fecaca" : "#94a3b8",
                }}>
                  {fileStatusLabel(file.status)}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Bottom action bar */}
      {selected.size > 0 && (
        <div style={{
          flexShrink: 0,
          padding: "12px",
          borderTop: "1px solid var(--border)",
          display: "flex",
          alignItems: "center",
          gap: "12px",
          background: "var(--bg-secondary)",
        }}>
          <span style={{ color: "var(--text-secondary)", fontSize: "0.875rem" }}>
            {selected.size} file{selected.size !== 1 ? "s" : ""} selected ·{" "}
            {formatBytes(
              filtered
                .filter((f) => selected.has(f.id))
                .reduce((sum, f) => sum + f.size_bytes, 0)
            )}
          </span>
          <button className="btn btn-primary" onClick={handleRecover}>
            ↗ Recover Selected
          </button>
          <button className="btn btn-ghost" onClick={clearAll}>
            Clear Selection
          </button>
        </div>
      )}
    </div>
  );
}
