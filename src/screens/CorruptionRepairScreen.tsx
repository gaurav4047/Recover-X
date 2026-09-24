/**
 * CorruptionRepairScreen
 *
 * Lets the user:
 *  1. Add files / folders to scan for corruption.
 *  2. View a per-file corruption report (kind, description, repairability).
 *  3. Choose an output folder (optional) and repair selected files.
 *  4. View repair results inline.
 */

import { useState, useCallback } from "react";
import {
  api,
  type CorruptionReport,
  type CorruptionKind,
  type RepairResult,
  corruptionKindLabel,
  corruptionKindColor,
  formatBytes,
  fileIcon,
} from "../lib/api";

// ── helpers ───────────────────────────────────────────────────────────────────

type Phase = "idle" | "scanning" | "scanned" | "repairing" | "done";

function badge(text: string, color: string) {
  return (
    <span style={{
      display: "inline-block",
      padding: "2px 8px",
      borderRadius: "4px",
      fontSize: "0.72rem",
      fontWeight: 600,
      background: `${color}22`,
      color,
      border: `1px solid ${color}55`,
    }}>
      {text}
    </span>
  );
}

function confidencePill(pct: number) {
  const color = pct >= 70 ? "#22c55e" : pct >= 40 ? "#f59e0b" : "#ef4444";
  return (
    <span style={{
      display: "inline-block",
      width: 36,
      textAlign: "center",
      padding: "2px 6px",
      borderRadius: "4px",
      fontSize: "0.72rem",
      fontWeight: 700,
      background: `${color}22`,
      color,
    }}>
      {pct}%
    </span>
  );
}

// ── main component ─────────────────────────────────────────────────────────────

export function CorruptionRepairScreen() {
  const [paths, setPaths] = useState<string[]>([]);
  const [inputPath, setInputPath] = useState("");

  const [phase, setPhase] = useState<Phase>("idle");
  const [isRepairing, setIsRepairing] = useState(false);
  const [reports, setReports] = useState<CorruptionReport[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [repairResults, setRepairResults] = useState<RepairResult[]>([]);
  const [error, setError] = useState<string | null>(null);

  // output folder — empty string = in-place repair
  const [outputDir, setOutputDir] = useState<string>("");
  const [isPickingFolder, setIsPickingFolder] = useState(false);

  // filter
  const [filterKind, setFilterKind] = useState<CorruptionKind | "all">("all");
  const [filterRepairableOnly, setFilterRepairableOnly] = useState(false);

  // ── path management ──────────────────────────────────────────────────────

  const addPath = () => {
    const t = inputPath.trim();
    if (!t || paths.includes(t)) return;
    setPaths(prev => [...prev, t]);
    setInputPath("");
  };

  const removePath = (p: string) =>
    setPaths(prev => prev.filter(x => x !== p));

  // ── scan ─────────────────────────────────────────────────────────────────

  const runScan = useCallback(async () => {
    if (paths.length === 0) {
      setError("Add at least one file or folder to scan.");
      return;
    }
    setError(null);
    setPhase("scanning");
    setReports([]);
    setSelected(new Set());
    setRepairResults([]);

    try {
      const result = await api.scanForCorruption({ paths });
      setReports(result);
      setPhase("scanned");

      // Auto-select all repairable & non-healthy files
      const autoSel = new Set(
        result
          .filter(r => r.repairable && r.corruption !== "healthy")
          .map(r => r.path)
      );
      setSelected(autoSel);
    } catch (e) {
      setError(String(e));
      setPhase("idle");
    }
  }, [paths]);

  // ── output folder picker ──────────────────────────────────────────────────

  const pickOutputFolder = useCallback(async () => {
    setIsPickingFolder(true);
    try {
      const chosen = await api.openFolderDialog();
      if (chosen) setOutputDir(chosen);
    } catch (e) {
      setError(String(e));
    } finally {
      setIsPickingFolder(false);
    }
  }, []);

  const clearOutputDir = () => setOutputDir("");

  // ── repair ────────────────────────────────────────────────────────────────

  const runRepair = useCallback(async () => {
    if (selected.size === 0) {
      setError("Select at least one file to repair.");
      return;
    }
    setError(null);
    setPhase("repairing");
    setIsRepairing(true);

    const toRepair = reports.filter(r => selected.has(r.path));
    try {
      const results = await api.repairCorruptedFiles({
        reports: toRepair,
        output_dir: outputDir,
      });
      setRepairResults(results);
      setPhase("done");
    } catch (e) {
      setError(String(e));
      setPhase("scanned");
    } finally {
      setIsRepairing(false);
    }
  }, [selected, reports, outputDir]);

  // ── selection helpers ─────────────────────────────────────────────────────

  const toggleSelect = (path: string) =>
    setSelected(prev => {
      const next = new Set(prev);
      next.has(path) ? next.delete(path) : next.add(path);
      return next;
    });

  const selectAll = () =>
    setSelected(new Set(filteredReports.filter(r => r.repairable).map(r => r.path)));

  const clearAll = () => setSelected(new Set());

  // ── derived state ─────────────────────────────────────────────────────────

  const filteredReports = reports.filter(r => {
    if (filterKind !== "all" && r.corruption !== filterKind) return false;
    if (filterRepairableOnly && !r.repairable) return false;
    return true;
  });

  const stats = {
    total:     reports.length,
    healthy:   reports.filter(r => r.corruption === "healthy").length,
    corrupted: reports.filter(r => r.corruption !== "healthy" && r.corruption !== "unknown").length,
    repairable:reports.filter(r => r.repairable).length,
    unknown:   reports.filter(r => r.corruption === "unknown").length,
  };

  const repairSucceeded = repairResults.filter(r => r.success).length;
  const repairFailed    = repairResults.filter(r => !r.success).length;

  const canScan   = phase === "idle" || phase === "scanned" || phase === "done";
  const canRepair = (phase === "scanned" || phase === "done") && selected.size > 0;
  const savingToDir = outputDir.trim().length > 0;

  const kindFilters: { kind: CorruptionKind | "all"; label: string }[] = [
    { kind: "all",               label: "All" },
    { kind: "bad_header",        label: "Bad Header" },
    { kind: "bad_footer",        label: "Missing Footer" },
    { kind: "broken_structure",  label: "Broken Structure" },
    { kind: "truncated_content", label: "Truncated" },
    { kind: "zero_padded_tail",  label: "Zero Padding" },
    { kind: "healthy",           label: "Healthy" },
  ];

  // ── render ────────────────────────────────────────────────────────────────

  return (
    <div style={{
      display: "flex",
      flexDirection: "column",
      height: "100%",
      overflow: "hidden",
      color: "var(--text-primary)",
    }}>
      {/* ── Header ── */}
      <div style={{
        padding: "20px 24px 16px",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 4 }}>
          <span style={{ fontSize: "1.4rem" }}>🛠️</span>
          <h1 style={{ margin: 0, fontSize: "1.25rem", fontWeight: 700 }}>
            Corrupted Data Recovery
          </h1>
        </div>
        <p style={{ margin: 0, fontSize: "0.85rem", color: "var(--text-muted)" }}>
          Scan files for structural corruption and automatically repair JPEG, PNG, PDF,
          ZIP, DOCX, WAV, MP4, SQLite and more.
        </p>
      </div>

      {/* ── Body (scrollable) ── */}
      <div style={{
        flex: 1, overflowY: "auto", padding: "20px 24px",
        display: "flex", flexDirection: "column", gap: 20,
      }}>

        {/* ── Step 1 – Add files / folders ── */}
        <section style={cardStyle}>
          <SectionTitle step={1} title="Add Files or Folders to Scan" />

          <div style={{ display: "flex", gap: 8, marginBottom: 10 }}>
            <input
              value={inputPath}
              onChange={e => setInputPath(e.target.value)}
              onKeyDown={e => e.key === "Enter" && addPath()}
              placeholder="Paste a file or folder path and press Enter…"
              style={inputStyle}
            />
            <button onClick={addPath} style={btnPrimary} disabled={!inputPath.trim()}>
              Add
            </button>
          </div>

          {paths.length === 0 ? (
            <EmptyHint>No paths added yet. Paste a path above (e.g. /Users/you/Documents).</EmptyHint>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              {paths.map(p => (
                <div key={p} style={{
                  display: "flex", alignItems: "center", gap: 8,
                  padding: "6px 10px", background: "var(--bg-primary)",
                  borderRadius: 6, border: "1px solid var(--border)",
                }}>
                  <span style={{
                    fontSize: "0.85rem", flex: 1,
                    overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                    color: "var(--text-secondary)",
                  }}>
                    📁 {p}
                  </span>
                  <button onClick={() => removePath(p)} style={btnGhost} title="Remove">✕</button>
                </div>
              ))}
            </div>
          )}

          <div style={{ marginTop: 14, display: "flex", gap: 8, alignItems: "center" }}>
            <button
              onClick={runScan}
              disabled={!canScan || paths.length === 0}
              style={phase === "scanning" ? btnDisabled : btnPrimary}
            >
              {phase === "scanning" ? "⏳ Scanning…" : "🔍 Scan for Corruption"}
            </button>
            {(phase === "scanned" || phase === "done") && (
              <span style={{ fontSize: "0.8rem", color: "var(--text-muted)" }}>
                Scanned {stats.total} file{stats.total !== 1 ? "s" : ""}
              </span>
            )}
          </div>
        </section>

        {/* ── Error banner ── */}
        {error && (
          <div style={{
            padding: "10px 14px", borderRadius: 6, fontSize: "0.85rem",
            background: "#ef444422", border: "1px solid #ef4444", color: "#ef4444",
          }}>
            ⚠️ {error}
          </div>
        )}

        {/* ── Step 2 – Scan Results ── */}
        {(phase === "scanned" || phase === "repairing" || phase === "done") && reports.length > 0 && (
          <section style={cardStyle}>
            <SectionTitle step={2} title="Scan Results" />

            {/* Stats row */}
            <div style={{ display: "flex", gap: 12, flexWrap: "wrap", marginBottom: 14 }}>
              <Stat label="Total"     value={stats.total}     color="var(--text-primary)" />
              <Stat label="Healthy"   value={stats.healthy}   color="#22c55e" />
              <Stat label="Corrupted" value={stats.corrupted} color="#ef4444" />
              <Stat label="Repairable"value={stats.repairable}color="#3b82f6" />
              <Stat label="Unknown"   value={stats.unknown}   color="#6b7280" />
            </div>

            {/* Warn if scanning a previously-repaired file */}
            {reports.some(r => {
              const name = r.path.split("/").pop() ?? "";
              return name.includes("_repaired") || name.includes("_backup");
            }) && (
              <div style={{
                padding: "10px 14px", borderRadius: 6, marginBottom: 10,
                background: "#f59e0b18", border: "1px solid #f59e0b55",
                fontSize: "0.82rem", color: "#f59e0b",
              }}>
                ⚠️ One or more files look like previous repair outputs (<code>_repaired</code> / <code>_backup</code>).
                Scan the <strong>original</strong> corrupted file, not a previous repair attempt.
              </div>
            )}

            {/* Filter bar */}
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginBottom: 12, alignItems: "center" }}>
              {kindFilters.map(({ kind, label }) => (
                <button
                  key={kind}
                  onClick={() => setFilterKind(kind)}
                  style={{
                    padding: "3px 10px", borderRadius: 20, fontSize: "0.78rem",
                    fontWeight: 500, cursor: "pointer", border: "1px solid var(--border)",
                    background: filterKind === kind ? "var(--accent, #6366f1)" : "var(--bg-primary)",
                    color: filterKind === kind ? "#fff" : "var(--text-secondary)",
                  }}
                >
                  {label}
                </button>
              ))}
              <label style={{
                display: "flex", alignItems: "center", gap: 5,
                fontSize: "0.8rem", color: "var(--text-secondary)", marginLeft: 4,
              }}>
                <input
                  type="checkbox"
                  checked={filterRepairableOnly}
                  onChange={e => setFilterRepairableOnly(e.target.checked)}
                />
                Repairable only
              </label>
              <div style={{ marginLeft: "auto", display: "flex", gap: 6 }}>
                <button onClick={selectAll} style={btnGhost}>Select repairable</button>
                <button onClick={clearAll}  style={btnGhost}>Deselect all</button>
              </div>
            </div>

            {/* File list */}
            <div style={{ display: "flex", flexDirection: "column", gap: 4, maxHeight: 360, overflowY: "auto" }}>
              {filteredReports.length === 0 ? (
                <EmptyHint>No files match the current filter.</EmptyHint>
              ) : (
                filteredReports.map(r => {
                  const isSelected = selected.has(r.path);
                  const kindColor  = corruptionKindColor(r.corruption);
                  return (
                    <div
                      key={r.path}
                      onClick={() => r.repairable && toggleSelect(r.path)}
                      style={{
                        display: "grid",
                        gridTemplateColumns: "20px 1fr auto",
                        alignItems: "center",
                        gap: 10,
                        padding: "8px 10px",
                        borderRadius: 6,
                        border: `1px solid ${isSelected ? "#6366f155" : "var(--border)"}`,
                        background: isSelected ? "#6366f108" : "var(--bg-primary)",
                        cursor: r.repairable ? "pointer" : "default",
                        opacity: r.repairable ? 1 : 0.65,
                        transition: "background 0.1s",
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={isSelected}
                        disabled={!r.repairable}
                        onChange={() => toggleSelect(r.path)}
                        onClick={e => e.stopPropagation()}
                        style={{ accentColor: "#6366f1" }}
                      />
                      <div style={{ minWidth: 0 }}>
                        <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 2 }}>
                          <span style={{ fontSize: "1rem" }}>{fileIcon(r.format || null)}</span>
                          <span style={{
                            fontSize: "0.82rem", fontWeight: 600,
                            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                          }}>
                            {r.path.split("/").pop() || r.path}
                          </span>
                          <span style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
                            {r.format.toUpperCase()} · {formatBytes(r.size_bytes)}
                          </span>
                        </div>
                        <div style={{
                          fontSize: "0.78rem", color: "var(--text-muted)",
                          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                        }}>
                          {r.path}
                        </div>
                        <div style={{ marginTop: 4, fontSize: "0.78rem", color: "var(--text-secondary)" }}>
                          {r.description}
                        </div>
                      </div>
                      <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 4, flexShrink: 0 }}>
                        {badge(corruptionKindLabel(r.corruption), kindColor)}
                        {r.repairable && r.corruption !== "healthy" && (
                          <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                            <span style={{ fontSize: "0.72rem", color: "var(--text-muted)" }}>Confidence</span>
                            {confidencePill(r.repair_confidence)}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </section>
        )}

        {/* ── Step 3 – Output folder + Repair ── */}
        {(phase === "scanned" || phase === "done") && stats.repairable > 0 && (
          <section style={cardStyle}>
            <SectionTitle step={3} title="Choose Output & Repair" />

            {/* ── Output folder picker ── */}
            <div style={{ marginBottom: 16 }}>
              <div style={{
                fontSize: "0.82rem", fontWeight: 600,
                color: "var(--text-secondary)", marginBottom: 8,
              }}>
                Output Folder
              </div>

              {/* Picker row */}
              <div style={{ display: "flex", gap: 8, alignItems: "stretch" }}>
                {/* Path display / input */}
                <div style={{
                  flex: 1,
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  padding: "8px 12px",
                  borderRadius: 6,
                  border: `1px solid ${savingToDir ? "#6366f155" : "var(--border)"}`,
                  background: savingToDir ? "#6366f108" : "var(--bg-primary)",
                  minWidth: 0,
                }}>
                  <span style={{ fontSize: "1rem", flexShrink: 0 }}>
                    {savingToDir ? "📂" : "📁"}
                  </span>
                  {savingToDir ? (
                    <>
                      <span style={{
                        flex: 1, fontSize: "0.83rem",
                        color: "var(--text-primary)",
                        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                      }}>
                        {outputDir}
                      </span>
                      <button
                        onClick={clearOutputDir}
                        title="Clear — use in-place repair"
                        style={{
                          ...btnGhost,
                          padding: "2px 8px",
                          fontSize: "0.75rem",
                          flexShrink: 0,
                          lineHeight: 1.4,
                        }}
                      >
                        ✕ Clear
                      </button>
                    </>
                  ) : (
                    <span style={{ flex: 1, fontSize: "0.83rem", color: "var(--text-muted)" }}>
                      No output folder selected — repaired files will replace the originals in-place
                    </span>
                  )}
                </div>

                {/* Browse button */}
                <button
                  onClick={pickOutputFolder}
                  disabled={isPickingFolder}
                  style={{
                    ...btnPrimary,
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    whiteSpace: "nowrap",
                    flexShrink: 0,
                    opacity: isPickingFolder ? 0.6 : 1,
                    cursor: isPickingFolder ? "not-allowed" : "pointer",
                  }}
                >
                  {isPickingFolder ? "⏳" : "📂"} Browse…
                </button>
              </div>

              {/* Context hint */}
              <div style={{ marginTop: 8, fontSize: "0.78rem", color: "var(--text-muted)" }}>
                {savingToDir ? (
                  <>
                    ✅ Repaired copies will be saved to <strong style={{ color: "var(--text-secondary)" }}>{outputDir}</strong>.
                    {" "}Originals are not modified.
                  </>
                ) : (
                  <>
                    ⚠️ <strong>In-place mode:</strong> the original file will be overwritten.
                    A <code style={{ fontSize: "0.75rem" }}>_backup</code> copy is automatically
                    created beside each file before repair.
                    {" "}Select an output folder above to save repaired copies separately.
                  </>
                )}
              </div>
            </div>

            {/* Divider */}
            <div style={{ height: 1, background: "var(--border)", margin: "4px 0 16px" }} />

            {/* Repair button row */}
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <button
                onClick={runRepair}
                disabled={!canRepair || isRepairing}
                style={canRepair && !isRepairing ? btnSuccess : btnDisabled}
              >
                {isRepairing
                  ? "⏳ Repairing…"
                  : savingToDir
                    ? `🔧 Repair ${selected.size} file${selected.size !== 1 ? "s" : ""} → output folder`
                    : `🔧 Repair ${selected.size} file${selected.size !== 1 ? "s" : ""} in-place`}
              </button>
              {selected.size > 0 && !isRepairing && (
                <span style={{ fontSize: "0.8rem", color: "var(--text-muted)" }}>
                  {selected.size} file{selected.size !== 1 ? "s" : ""} selected
                </span>
              )}
            </div>
          </section>
        )}

        {/* ── Step 4 – Repair Results ── */}
        {phase === "done" && repairResults.length > 0 && (
          <section style={cardStyle}>
            <SectionTitle step={4} title="Repair Results" />

            <div style={{ display: "flex", gap: 12, marginBottom: 14 }}>
              <Stat label="Succeeded" value={repairSucceeded} color="#22c55e" />
              <Stat label="Failed"    value={repairFailed}    color="#ef4444" />
            </div>

            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              {repairResults.map((r, i) => (
                <div key={i} style={{
                  padding: "8px 12px", borderRadius: 6,
                  border: `1px solid ${r.success ? "#22c55e44" : "#ef444444"}`,
                  background: r.success ? "#22c55e0a" : "#ef44440a",
                }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 2 }}>
                    <span style={{ fontSize: "1rem" }}>{r.success ? "✅" : "❌"}</span>
                    <span style={{ fontWeight: 600, fontSize: "0.85rem" }}>
                      {r.original_path.split("/").pop() || r.original_path}
                    </span>
                    {r.success && (
                      <span style={{ fontSize: "0.78rem", color: "var(--text-muted)" }}>
                        → {formatBytes(r.bytes_written)} written
                      </span>
                    )}
                  </div>
                  <div style={{ fontSize: "0.78rem", color: "var(--text-secondary)", marginLeft: 28 }}>
                    {r.success ? r.action : r.error}
                  </div>
                  {r.success && (
                    <div style={{ fontSize: "0.75rem", marginLeft: 28, marginTop: 4 }}>
                      {savingToDir ? (
                        <span style={{ color: "#22c55e" }}>
                          ✅ Repaired copy saved to{" "}
                          <span style={{ color: "var(--text-secondary)", fontWeight: 600 }}>
                            {r.repaired_path || outputDir}
                          </span>
                          {" "}— original untouched
                        </span>
                      ) : (
                        <span style={{ color: "#22c55e" }}>
                          ✅ File repaired in-place — original backed up as _backup
                        </span>
                      )}
                    </div>
                  )}
                </div>
              ))}
            </div>

            <div style={{ marginTop: 14 }}>
              <button
                onClick={() => {
                  setPhase("idle");
                  setReports([]);
                  setRepairResults([]);
                  setSelected(new Set());
                }}
                style={btnGhost}
              >
                ↩ Start New Scan
              </button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function SectionTitle({ step, title }: { step: number; title: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
      <span style={{
        width: 22, height: 22, borderRadius: "50%", background: "#6366f1",
        color: "#fff", fontSize: "0.75rem", fontWeight: 700,
        display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0,
      }}>
        {step}
      </span>
      <span style={{ fontWeight: 600, fontSize: "0.95rem" }}>{title}</span>
    </div>
  );
}

function Stat({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div style={{
      display: "flex", flexDirection: "column", alignItems: "center",
      padding: "8px 14px", borderRadius: 8,
      border: "1px solid var(--border)", background: "var(--bg-primary)",
      minWidth: 72,
    }}>
      <span style={{ fontSize: "1.25rem", fontWeight: 800, color }}>{value}</span>
      <span style={{ fontSize: "0.72rem", color: "var(--text-muted)" }}>{label}</span>
    </div>
  );
}

function EmptyHint({ children }: { children: React.ReactNode }) {
  return (
    <div style={{
      textAlign: "center", padding: "20px 0",
      color: "var(--text-muted)", fontSize: "0.85rem",
    }}>
      {children}
    </div>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const cardStyle: React.CSSProperties = {
  background: "var(--bg-secondary)",
  border: "1px solid var(--border)",
  borderRadius: 10,
  padding: "16px 18px",
};

const inputStyle: React.CSSProperties = {
  flex: 1,
  padding: "8px 12px",
  fontSize: "0.875rem",
  borderRadius: 6,
  border: "1px solid var(--border)",
  background: "var(--bg-primary)",
  color: "var(--text-primary)",
  outline: "none",
};

const btnBase: React.CSSProperties = {
  padding: "8px 16px",
  borderRadius: 6,
  fontSize: "0.875rem",
  fontWeight: 600,
  cursor: "pointer",
  border: "none",
  transition: "opacity 0.1s",
};

const btnPrimary: React.CSSProperties = {
  ...btnBase,
  background: "#6366f1",
  color: "#fff",
};

const btnSuccess: React.CSSProperties = {
  ...btnBase,
  background: "#22c55e",
  color: "#fff",
};

const btnGhost: React.CSSProperties = {
  ...btnBase,
  background: "transparent",
  border: "1px solid var(--border)",
  color: "var(--text-secondary)",
};

const btnDisabled: React.CSSProperties = {
  ...btnBase,
  background: "var(--bg-tertiary, #333)",
  color: "var(--text-muted)",
  cursor: "not-allowed",
};
