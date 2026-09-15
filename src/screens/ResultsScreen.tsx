import { useCallback, useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, formatBytes, confidenceColor, fileStatusLabel, fileIcon } from "../lib/api";
import type { RecoveredFile, ScanSession } from "../lib/api";

interface LocationState {
  sessionId?: string;
  session?: ScanSession;
}

const PAGE_SIZE = 100;

const CATEGORY_ICONS: Record<string, string> = {
  All: "📁",
  Images: "🖼️",
  Videos: "🎬",
  Audio: "🎵",
  Documents: "📄",
  Archives: "📦",
  Databases: "🗄️",
  Code: "💻",
  Other: "📎",
};

export function ResultsScreen() {
  const location = useLocation();
  const navigate = useNavigate();
  const locState = (location.state as LocationState) ?? {};

  const [sessionId, setSessionId] = useState<string | null>(
    locState.sessionId ?? locState.session?.id ?? null
  );

  // Category tabs
  const [categories, setCategories] = useState<[string, number][]>([]);
  const [activeCategory, setActiveCategory] = useState<string>("All");

  // File list
  const [files, setFiles] = useState<RecoveredFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [offset, setOffset] = useState(0);
  const [error, setError] = useState<string | null>(null);

  // Search
  const [search, setSearch] = useState("");
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Selection
  const [selected, setSelected] = useState<Set<string>>(new Set());

  // Scroll container ref for infinite scroll
  const scrollRef = useRef<HTMLDivElement>(null);

  // Load most recent completed session if none specified
  useEffect(() => {
    if (!sessionId) {
      api.listSessions().then((sessions) => {
        const completed = sessions.find((s) => s.status === "completed");
        if (completed) setSessionId(completed.id);
      }).catch(() => {});
    }
  }, []);

  // Load category counts whenever sessionId changes
  useEffect(() => {
    if (!sessionId) return;
    api.getCategoryCounts(sessionId).then((counts) => {
      setCategories(counts);
    }).catch(() => {});
  }, [sessionId]);

  // Reset and reload when category or search changes
  useEffect(() => {
    if (!sessionId) return;
    setFiles([]);
    setOffset(0);
    setHasMore(true);
    setSelected(new Set());
    loadPage(0, activeCategory, search);
  }, [sessionId, activeCategory]);

  // Debounced search
  const handleSearch = (value: string) => {
    setSearch(value);
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(() => {
      setFiles([]);
      setOffset(0);
      setHasMore(true);
      setSelected(new Set());
      loadPage(0, activeCategory, value);
    }, 300);
  };

  const loadPage = useCallback(async (
    pageOffset: number,
    category: string,
    searchVal: string
  ) => {
    if (!sessionId) return;
    if (pageOffset === 0) setLoading(true); else setLoadingMore(true);
    setError(null);

    try {
      const cat = category === "All" ? null : category;
      const srch = searchVal.trim() || null;
      const page = await api.queryRecoveredFiles(sessionId, cat, srch, PAGE_SIZE, pageOffset);
      setFiles((prev) => pageOffset === 0 ? page : [...prev, ...page]);
      setOffset(pageOffset + page.length);
      setHasMore(page.length === PAGE_SIZE);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setLoadingMore(false);
    }
  }, [sessionId]);

  // Infinite scroll handler
  const handleScroll = useCallback(() => {
    if (!scrollRef.current || loadingMore || !hasMore) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    if (scrollHeight - scrollTop - clientHeight < 200) {
      loadPage(offset, activeCategory, search);
    }
  }, [offset, activeCategory, search, loadingMore, hasMore, loadPage]);

  const toggleSelect = (id: string) => {
    setSelected((prev) => {
      const s = new Set(prev);
      s.has(id) ? s.delete(id) : s.add(id);
      return s;
    });
  };

  const selectAll = () => setSelected(new Set(files.map((f) => f.id)));
  const clearAll = () => setSelected(new Set());

  const handleRecover = () => {
    if (!sessionId || selected.size === 0) return;
    navigate("/recovery", { state: { sessionId, fileIds: Array.from(selected) } });
  };

  const totalInCategory = categories.find(([c]) => c === activeCategory)?.[1] ?? 0;

  if (!sessionId) {
    return (
      <div style={{ padding: "24px", height: "100%", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center" }}>
        <div className="empty-state">
          <div className="empty-icon">📋</div>
          <h3>No scan results yet</h3>
          <p>Run a scan to find recoverable files.</p>
          <button className="btn btn-primary" style={{ marginTop: "20px" }} onClick={() => navigate("/scan-setup")}>
            Start a Scan
          </button>
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>

      {/* ── Header ── */}
      <div style={{ padding: "16px 20px 0", flexShrink: 0 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "12px" }}>
          <div>
            <h2 style={{ fontSize: "1.25rem" }}>Recovery Results</h2>
            <p style={{ color: "var(--text-secondary)", fontSize: "0.8125rem", marginTop: "2px" }}>
              {loading ? "Loading…" : `${totalInCategory.toLocaleString()} files · ${files.length.toLocaleString()} loaded`}
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

        {error && <div className="alert alert-error" style={{ marginBottom: "8px" }}>{error}</div>}

        {/* ── Category tabs ── */}
        <div style={{ display: "flex", gap: "6px", overflowX: "auto", paddingBottom: "4px" }}>
          {categories.map(([cat, count]) => (
            <button
              key={cat}
              onClick={() => setActiveCategory(cat)}
              style={{
                padding: "6px 14px",
                background: activeCategory === cat ? "var(--accent-blue)" : "var(--bg-secondary)",
                border: `1px solid ${activeCategory === cat ? "var(--accent-blue)" : "var(--border)"}`,
                borderRadius: "20px",
                color: activeCategory === cat ? "#fff" : "var(--text-secondary)",
                cursor: "pointer",
                fontSize: "0.8125rem",
                fontWeight: activeCategory === cat ? 600 : 400,
                whiteSpace: "nowrap",
                display: "flex",
                alignItems: "center",
                gap: "5px",
                flexShrink: 0,
              }}
            >
              <span>{CATEGORY_ICONS[cat] ?? "📎"}</span>
              {cat}
              <span style={{
                background: activeCategory === cat ? "rgba(255,255,255,0.25)" : "var(--bg-tertiary)",
                borderRadius: "10px",
                padding: "1px 6px",
                fontSize: "0.7rem",
                fontWeight: 700,
              }}>
                {count.toLocaleString()}
              </span>
            </button>
          ))}
        </div>

        {/* ── Search + select controls ── */}
        <div style={{ display: "flex", gap: "8px", margin: "10px 0 8px", alignItems: "center" }}>
          <input
            className="form-input"
            style={{ maxWidth: "280px" }}
            type="text"
            placeholder="Search by filename…"
            value={search}
            onChange={(e) => handleSearch(e.target.value)}
          />
          <button className="btn btn-ghost" style={{ fontSize: "0.8125rem" }} onClick={selectAll}>
            Select All
          </button>
          {selected.size > 0 && (
            <button className="btn btn-ghost" style={{ fontSize: "0.8125rem" }} onClick={clearAll}>
              Clear
            </button>
          )}
        </div>

        {/* ── Table header ── */}
        <div style={{
          display: "grid",
          gridTemplateColumns: "32px 1fr 90px 90px 110px",
          gap: "8px",
          padding: "6px 10px",
          fontSize: "0.7rem",
          color: "var(--text-muted)",
          textTransform: "uppercase",
          letterSpacing: "0.06em",
          borderBottom: "1px solid var(--border)",
        }}>
          <span />
          <span>Name / Path</span>
          <span>Size</span>
          <span>Confidence</span>
          <span>Status</span>
        </div>
      </div>

      {/* ── File list (scrollable) ── */}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        style={{ flex: 1, overflow: "auto", padding: "0 20px" }}
      >
        {loading && (
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", padding: "40px", color: "var(--text-muted)", gap: "10px" }}>
            <div style={{ width: "16px", height: "16px", border: "2px solid var(--accent-blue)", borderTopColor: "transparent", borderRadius: "50%", animation: "spin 0.7s linear infinite" }} />
            Loading files…
          </div>
        )}

        {!loading && files.length === 0 && (
          <div className="empty-state" style={{ padding: "60px 20px" }}>
            <div className="empty-icon">📋</div>
            <h3>No files found</h3>
            <p>{search ? "Try a different search term." : "No files in this category."}</p>
          </div>
        )}

        {files.map((file) => (
          <FileRow
            key={file.id}
            file={file}
            selected={selected.has(file.id)}
            onToggle={() => toggleSelect(file.id)}
          />
        ))}

        {loadingMore && (
          <div style={{ textAlign: "center", padding: "16px", color: "var(--text-muted)", fontSize: "0.8125rem" }}>
            Loading more…
          </div>
        )}

        {!hasMore && files.length > 0 && (
          <div style={{ textAlign: "center", padding: "16px", color: "var(--text-muted)", fontSize: "0.75rem" }}>
            All {files.length.toLocaleString()} files loaded
          </div>
        )}
      </div>

      {/* ── Bottom action bar ── */}
      {selected.size > 0 && (
        <div style={{
          flexShrink: 0,
          padding: "12px 20px",
          borderTop: "1px solid var(--border)",
          display: "flex",
          alignItems: "center",
          gap: "12px",
          background: "var(--bg-secondary)",
        }}>
          <span style={{ color: "var(--text-secondary)", fontSize: "0.875rem" }}>
            {selected.size} file{selected.size !== 1 ? "s" : ""} selected ·{" "}
            {formatBytes(files.filter((f) => selected.has(f.id)).reduce((s, f) => s + f.size_bytes, 0))}
          </span>
          <button className="btn btn-primary" onClick={handleRecover}>
            ↗ Recover Selected
          </button>
          <button className="btn btn-ghost" onClick={clearAll}>Clear</button>
        </div>
      )}

      <style>{`
        @keyframes spin { to { transform: rotate(360deg); } }
      `}</style>
    </div>
  );
}

// ── Single file row (memoized for performance) ────────────────────────────────

interface FileRowProps {
  file: RecoveredFile;
  selected: boolean;
  onToggle: () => void;
}

function FileRow({ file, selected, onToggle }: FileRowProps) {
  return (
    <div
      onClick={onToggle}
      style={{
        display: "grid",
        gridTemplateColumns: "32px 1fr 90px 90px 110px",
        gap: "8px",
        padding: "7px 10px",
        borderBottom: "1px solid var(--border)",
        background: selected ? "rgba(59,130,246,0.08)" : "transparent",
        cursor: "pointer",
        alignItems: "center",
        fontSize: "0.8125rem",
      }}
    >
      <input
        type="checkbox"
        checked={selected}
        onChange={onToggle}
        onClick={(e) => e.stopPropagation()}
        style={{ accentColor: "var(--accent-blue)", cursor: "pointer" }}
      />

      <div style={{ minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          <span style={{ fontSize: "1rem", flexShrink: 0 }}>{fileIcon(file.extension)}</span>
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: 500 }}>
            {file.name}
          </span>
          {!file.original_path && (
            <span style={{ fontSize: "0.65rem", color: "var(--text-muted)", flexShrink: 0 }}>carved</span>
          )}
        </div>
        <div style={{ fontSize: "0.7rem", color: "var(--text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", marginTop: "1px" }}>
          {file.original_path ?? (file.filesystem_type ?? file.recovery_method)}
        </div>
      </div>

      <div style={{ color: "var(--text-secondary)" }}>
        {formatBytes(file.size_bytes)}
      </div>

      <div style={{ color: confidenceColor(file.confidence), fontWeight: 600 }}>
        {file.confidence}%
      </div>

      <div>
        <span style={{
          fontSize: "0.7rem",
          padding: "2px 7px",
          borderRadius: "4px",
          background:
            file.status === "complete" ? "#052e16" :
            file.status === "partial"  ? "#451a03" :
            file.status === "corrupted"? "#450a0a" : "#1e293b",
          color:
            file.status === "complete" ? "#bbf7d0" :
            file.status === "partial"  ? "#fde68a" :
            file.status === "corrupted"? "#fecaca" : "#94a3b8",
          whiteSpace: "nowrap",
        }}>
          {fileStatusLabel(file.status)}
        </span>
      </div>
    </div>
  );
}
