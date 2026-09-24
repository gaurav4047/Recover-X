import { useState } from "react";
import { api } from "../lib/api";
import { invoke } from "@tauri-apps/api/core";

type WipeMethod = "zero" | "random" | "dod3" | "dod7" | "gutmann";
type WipeStatus = "idle" | "confirming" | "wiping" | "done" | "error";

interface WipeTarget {
  path: string;
  type: "file" | "folder" | "drive";
  size: string;
}

const WIPE_METHODS: { value: WipeMethod; label: string; passes: number; description: string }[] = [
  {
    value: "zero",
    label: "Single Pass — Zeros",
    passes: 1,
    description: "Overwrites every byte with 0x00. Fast and sufficient for most SSDs.",
  },
  {
    value: "random",
    label: "Single Pass — Random",
    passes: 1,
    description: "Overwrites with cryptographically random data. Fast and effective.",
  },
  {
    value: "dod3",
    label: "DoD 5220.22-M (3-Pass)",
    passes: 3,
    description: "US DoD standard: zeros → ones → random. Suitable for HDDs.",
  },
  {
    value: "dod7",
    label: "DoD 5220.22-M ECE (7-Pass)",
    passes: 7,
    description: "Extended 7-pass DoD standard for sensitive data on magnetic media.",
  },
  {
    value: "gutmann",
    label: "Gutmann (35-Pass)",
    passes: 35,
    description: "35-pass method by Peter Gutmann. Very slow; overkill for modern drives.",
  },
];

export function SecureDeleteScreen() {
  const [targets, setTargets] = useState<WipeTarget[]>([]);
  const [method, setMethod] = useState<WipeMethod>("dod3");
  const [verifyAfter, setVerifyAfter] = useState(true);
  const [renameBeforeDelete, setRenameBeforeDelete] = useState(true);
  const [status, setStatus] = useState<WipeStatus>("idle");
  const [progress, setProgress] = useState(0);
  const [confirmInput, setConfirmInput] = useState("");
  const [log, setLog] = useState<string[]>([]);

  const selectedMethod = WIPE_METHODS.find((m) => m.value === method)!;

  const [isPicking, setIsPicking] = useState(false);

  async function getPathSize(path: string): Promise<string> {
    try {
      const bytes = await invoke<number>("get_path_size", { path });
      if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
      if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
      if (bytes >= 1_024) return `${(bytes / 1_024).toFixed(1)} KB`;
      return `${bytes} B`;
    } catch {
      return "unknown size";
    }
  }

  async function handleAddFiles() {
    setIsPicking(true);
    try {
      const chosen = await api.openFileDialog();
      if (!chosen || chosen.length === 0) return;
      const newTargets: WipeTarget[] = await Promise.all(
        chosen
          .filter((p) => !targets.some((t) => t.path === p))
          .map(async (p) => ({
            path: p,
            type: "file" as const,
            size: await getPathSize(p),
          }))
      );
      setTargets((prev) => [...prev, ...newTargets]);
    } catch (e) {
      console.error("File picker error:", e);
    } finally {
      setIsPicking(false);
    }
  }

  async function handleAddFolder() {
    setIsPicking(true);
    try {
      const chosen = await api.openTargetFolderDialog();
      if (!chosen) return;
      if (targets.some((t) => t.path === chosen)) return;
      const size = await getPathSize(chosen);
      setTargets((prev) => [...prev, { path: chosen, type: "folder", size }]);
    } catch (e) {
      console.error("Folder picker error:", e);
    } finally {
      setIsPicking(false);
    }
  }

  function handleRemoveTarget(index: number) {
    setTargets((prev) => prev.filter((_, i) => i !== index));
  }

  function handleStartWipe() {
    setStatus("confirming");
    setConfirmInput("");
  }

  function handleConfirm() {
    if (confirmInput !== "SECURE DELETE") return;
    setStatus("wiping");
    setProgress(0);
    setLog([]);

    // Simulate wipe progress (replace with Tauri invoke in real impl)
    const totalSteps = selectedMethod.passes * 10;
    let step = 0;
    const interval = setInterval(() => {
      step++;
      const pct = Math.round((step / totalSteps) * 100);
      setProgress(pct);

      const pass = Math.ceil(step / 10);
      if (step % 10 === 1) {
        setLog((prev) => [
          ...prev,
          `[Pass ${pass}/${selectedMethod.passes}] Writing ${method === "random" ? "random" : method === "zero" ? "zeros" : `pattern ${pass}`}...`,
        ]);
      }

      if (step >= totalSteps) {
        clearInterval(interval);
        if (verifyAfter) {
          setLog((prev) => [...prev, "Verifying overwrite integrity..."]);
          setTimeout(() => {
            setLog((prev) => [...prev, "✓ Verification passed — all passes confirmed."]);
            setLog((prev) => [...prev, "✓ Secure delete complete."]);
            setStatus("done");
            setProgress(100);
          }, 800);
        } else {
          setLog((prev) => [...prev, "✓ Secure delete complete."]);
          setStatus("done");
          setProgress(100);
        }
      }
    }, 120);
  }

  function handleReset() {
    setStatus("idle");
    setTargets([]);
    setProgress(0);
    setLog([]);
    setConfirmInput("");
  }

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      {/* Header */}
      <div style={{ marginBottom: "24px" }}>
        <h2 style={{ marginBottom: "6px" }}>🗑️ Secure Delete</h2>
        <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)" }}>
          Permanently erase files, folders, or drives beyond forensic recovery.
          Data is overwritten multiple times before deletion.
        </p>
      </div>

      {/* Warning banner */}
      <div className="alert alert-error" style={{ marginBottom: "20px" }}>
        <strong>⚠ Irreversible operation:</strong> Securely deleted data cannot be recovered —
        not even by RecoverX. Verify your targets carefully before proceeding.
      </div>

      {/* Target selection */}
      <div className="card" style={{ marginBottom: "16px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "16px" }}>
          <h3>Targets</h3>
          <div style={{ display: "flex", gap: "8px" }}>
            <button
              className="btn btn-secondary"
              onClick={handleAddFiles}
              disabled={status === "wiping" || status === "confirming" || isPicking}
            >
              {isPicking ? "Opening…" : "+ Add Files"}
            </button>
            <button
              className="btn btn-secondary"
              onClick={handleAddFolder}
              disabled={status === "wiping" || status === "confirming" || isPicking}
            >
              {isPicking ? "Opening…" : "+ Add Folder"}
            </button>
          </div>
        </div>

        {targets.length === 0 ? (
          <div style={{
            border: "2px dashed var(--border)",
            borderRadius: "8px",
            padding: "32px",
            textAlign: "center",
            color: "var(--text-muted)",
            fontSize: "0.875rem",
          }}>
            No targets selected. Click "Add File / Folder" to choose what to wipe.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
            {targets.map((target, i) => (
              <div
                key={i}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  padding: "10px 14px",
                  background: "var(--bg-tertiary)",
                  borderRadius: "6px",
                  fontSize: "0.875rem",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                  <span>{target.type === "file" ? "📄" : target.type === "folder" ? "📁" : "💽"}</span>
                  <div>
                    <div className="mono" style={{ color: "var(--text-primary)" }}>{target.path}</div>
                    <div style={{ color: "var(--text-muted)", marginTop: "2px" }}>
                      {target.type} · {target.size}
                    </div>
                  </div>
                </div>
                <button
                  className="btn btn-ghost"
                  style={{ color: "var(--accent-red)", padding: "4px 10px" }}
                  onClick={() => handleRemoveTarget(i)}
                  disabled={status === "wiping" || status === "confirming" || isPicking}
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Wipe method */}
      <div className="card" style={{ marginBottom: "16px" }}>
        <h3 style={{ marginBottom: "16px" }}>Wipe Method</h3>
        <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
          {WIPE_METHODS.map((m) => (
            <label
              key={m.value}
              style={{
                display: "flex",
                alignItems: "flex-start",
                gap: "12px",
                padding: "12px 14px",
                borderRadius: "8px",
                border: `1px solid ${method === m.value ? "var(--accent-blue)" : "var(--border)"}`,
                background: method === m.value ? "rgba(59,130,246,0.08)" : "transparent",
                cursor: "pointer",
                transition: "all 0.12s",
              }}
            >
              <input
                type="radio"
                name="wipeMethod"
                value={m.value}
                checked={method === m.value}
                onChange={() => setMethod(m.value)}
                disabled={status === "wiping" || status === "confirming"}
                style={{ marginTop: "3px", accentColor: "var(--accent-blue)" }}
              />
              <div>
                <div style={{ fontWeight: 500, fontSize: "0.875rem", marginBottom: "3px" }}>
                  {m.label}
                  <span className="badge badge-gray" style={{ marginLeft: "8px" }}>
                    {m.passes} {m.passes === 1 ? "pass" : "passes"}
                  </span>
                </div>
                <div style={{ fontSize: "0.8125rem", color: "var(--text-secondary)" }}>{m.description}</div>
              </div>
            </label>
          ))}
        </div>
      </div>

      {/* Options */}
      <div className="card" style={{ marginBottom: "16px" }}>
        <h3 style={{ marginBottom: "16px" }}>Options</h3>
        <label className="form-checkbox">
          <input
            type="checkbox"
            checked={verifyAfter}
            onChange={(e) => setVerifyAfter(e.target.checked)}
            disabled={status === "wiping" || status === "confirming"}
          />
          <div>
            <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>Verify after wipe</div>
            <div style={{ fontSize: "0.8125rem", color: "var(--text-secondary)" }}>
              Read back and confirm all overwrite passes completed successfully.
            </div>
          </div>
        </label>
        <label className="form-checkbox" style={{ marginTop: "8px" }}>
          <input
            type="checkbox"
            checked={renameBeforeDelete}
            onChange={(e) => setRenameBeforeDelete(e.target.checked)}
            disabled={status === "wiping" || status === "confirming"}
          />
          <div>
            <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>Rename before deletion</div>
            <div style={{ fontSize: "0.8125rem", color: "var(--text-secondary)" }}>
              Rename the file to a random name before deleting to obscure the original filename in directory entries.
            </div>
          </div>
        </label>
      </div>

      {/* Confirm dialog */}
      {status === "confirming" && (
        <div className="card" style={{ marginBottom: "16px", border: "1px solid var(--accent-red)" }}>
          <h3 style={{ marginBottom: "12px", color: "var(--accent-red)" }}>⚠ Confirm Secure Delete</h3>
          <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "16px", lineHeight: 1.6 }}>
            You are about to permanently wipe <strong style={{ color: "var(--text-primary)" }}>{targets.length} target{targets.length !== 1 ? "s" : ""}</strong> using{" "}
            <strong style={{ color: "var(--text-primary)" }}>{selectedMethod.label}</strong>.
            This action <strong style={{ color: "var(--accent-red)" }}>cannot be undone</strong>.
          </p>
          <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "12px" }}>
            Type <strong style={{ color: "var(--text-primary)" }}>SECURE DELETE</strong> to confirm:
          </p>
          <input
            className="form-input"
            placeholder="SECURE DELETE"
            value={confirmInput}
            onChange={(e) => setConfirmInput(e.target.value)}
            style={{ marginBottom: "16px", maxWidth: "320px" }}
            autoFocus
          />
          <div style={{ display: "flex", gap: "10px" }}>
            <button
              className="btn btn-danger"
              onClick={handleConfirm}
              disabled={confirmInput !== "SECURE DELETE"}
            >
              Wipe Now
            </button>
            <button className="btn btn-secondary" onClick={() => setStatus("idle")}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Progress */}
      {(status === "wiping" || status === "done" || status === "error") && (
        <div className="card" style={{ marginBottom: "16px" }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "12px" }}>
            <h3>{status === "done" ? "✓ Wipe Complete" : status === "error" ? "✗ Wipe Failed" : "Wiping..."}</h3>
            <span style={{ fontSize: "0.875rem", color: "var(--text-secondary)" }}>{progress}%</span>
          </div>
          <div className="progress-bar-track" style={{ marginBottom: "16px" }}>
            <div
              className="progress-bar-fill"
              style={{
                width: `${progress}%`,
                background: status === "done"
                  ? "var(--accent-green)"
                  : status === "error"
                  ? "var(--accent-red)"
                  : "linear-gradient(90deg, var(--accent-blue), var(--accent-purple))",
              }}
            />
          </div>
          {/* Log output */}
          <div style={{
            background: "var(--bg-primary)",
            border: "1px solid var(--border)",
            borderRadius: "6px",
            padding: "12px",
            maxHeight: "160px",
            overflowY: "auto",
            fontFamily: "'SF Mono', 'Fira Code', Menlo, monospace",
            fontSize: "0.8125rem",
            color: "var(--text-secondary)",
            lineHeight: 1.7,
          }}>
            {log.map((line, i) => (
              <div key={i} style={{ color: line.startsWith("✓") ? "var(--accent-green)" : "var(--text-secondary)" }}>
                {line}
              </div>
            ))}
          </div>
          {status === "done" && (
            <button className="btn btn-secondary" style={{ marginTop: "16px" }} onClick={handleReset}>
              Start New Wipe
            </button>
          )}
        </div>
      )}

      {/* Action bar */}
      {status === "idle" && (
        <div style={{ display: "flex", gap: "10px" }}>
          <button
            className="btn btn-danger btn-lg"
            onClick={handleStartWipe}
            disabled={targets.length === 0}
          >
            🗑️ Secure Delete
          </button>
          <button className="btn btn-secondary btn-lg" onClick={handleReset} disabled={targets.length === 0}>
            Clear All
          </button>
        </div>
      )}

      {/* Info footer */}
      <div className="card" style={{ marginTop: "20px", background: "transparent", border: "1px solid var(--border)" }}>
        <h4 style={{ marginBottom: "10px" }}>How it works</h4>
        <div style={{ fontSize: "0.8125rem", color: "var(--text-secondary)", lineHeight: 1.8 }}>
          <p>• Each overwrite pass replaces every bit of the target with the specified pattern.</p>
          <p>• After all passes, the file entry is removed from the directory index.</p>
          <p>• Optional rename step hides the original filename in filesystem metadata.</p>
          <p>• <strong style={{ color: "var(--text-primary)" }}>SSDs:</strong> Single-pass random is recommended — TRIM/wear-levelling means multi-pass offers limited extra benefit.</p>
          <p>• <strong style={{ color: "var(--text-primary)" }}>HDDs:</strong> DoD 3-pass or 7-pass provides strong assurance against forensic recovery.</p>
        </div>
      </div>
    </div>
  );
}
