import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, formatBytes } from "../lib/api";
import type { ScanSession, ScanProgressPayload } from "../lib/api";

interface LocationState {
  session?: ScanSession;
}

const PIPELINE_STAGES = [
  "Source Validation",
  "Partition Detection",
  "Filesystem Detection",
  "Filesystem Metadata",
  "Deleted File Analysis",
  "File Carving",
  "Result Normalization",
  "Duplicate Detection",
  "Confidence Scoring",
  "Indexing Results",
];

export function ScanningScreen() {
  const navigate = useNavigate();
  const location = useLocation();
  const locState = (location.state as LocationState) ?? {};

  const [session, setSession] = useState<ScanSession | null>(locState.session ?? null);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [currentStage, setCurrentStage] = useState("");
  const [progress, setProgress] = useState(0);
  const [speed, setSpeed] = useState(0);
  const [eta, setEta] = useState<number | null>(null);
  const [filesFound, setFilesFound] = useState(0);
  const [badSectors, setBadSectors] = useState(0);
  const [processedBytes, setProcessedBytes] = useState(0);
  const [totalBytes, setTotalBytes] = useState(0);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);

  const unlistenRef = useRef<(() => void) | null>(null);

  // Subscribe to real-time scan events from the Rust backend
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    api.onScanEvent((event: ScanProgressPayload) => {
      if (event.type === "scan_progress") {
        setProgress(event.percent ?? 0);
        setSpeed(event.speed_bps ?? 0);
        setEta(event.eta_seconds ?? null);
        setFilesFound(event.files_found ?? 0);
        setBadSectors(event.bad_sectors ?? 0);
        setProcessedBytes(event.processed_bytes ?? 0);
        setTotalBytes(event.total_bytes ?? 0);
        if (event.current_stage) setCurrentStage(event.current_stage);
      } else if (event.type === "task_state_changed") {
        if (event.new_status === "running" && event.task_type) {
          setCurrentStage(event.task_type);
        }
      } else if (event.type === "scan_completed") {
        setProgress(100);
        setCurrentStage("Complete");
        setFilesFound((prev) => event.files_found ?? prev);
        setScanning(false);
        // Reload session to get final state
        if (session) {
          api.loadSession(session.id).then(setSession).catch(() => {});
        }
      } else if (event.type === "scan_failed") {
        setError(event.error ?? "Scan failed");
        setScanning(false);
      } else if (event.type === "scan_started") {
        setCurrentStage("Starting…");
      }
    }).then((fn) => {
      unlisten = fn;
      unlistenRef.current = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [session?.id]);

  // Start scan on mount if session is in Created state
  useEffect(() => {
    if (!session) return;
    if (session.status === "created") {
      startScan();
    } else if (session.status === "running") {
      setScanning(true);
    }
  }, []);

  // Fallback polling (every 2s) to catch completed/failed if events are missed
  useEffect(() => {
    if (!session || !scanning) return;
    const interval = setInterval(async () => {
      try {
        const updated = await api.loadSession(session.id);
        setSession(updated);
        if (updated.files_found > 0) setFilesFound(Number(updated.files_found));
        if (updated.bad_sectors > 0) setBadSectors(Number(updated.bad_sectors));
        if (updated.processed_bytes > 0) {
          setProcessedBytes(Number(updated.processed_bytes));
          setTotalBytes(Number(updated.source_size_bytes));
        }
        if (["completed", "failed", "cancelled"].includes(updated.status)) {
          setScanning(false);
          if (updated.status === "completed") {
            setProgress(100);
            setCurrentStage("Complete");
            setFilesFound(Number(updated.files_found));
          }
          clearInterval(interval);
        }
      } catch (_) {}
    }, 2000);
    return () => clearInterval(interval);
  }, [session?.id, scanning]);

  // Elapsed time
  useEffect(() => {
    if (!scanning) return;
    const interval = setInterval(() => setElapsedSeconds((s) => s + 1), 1000);
    return () => clearInterval(interval);
  }, [scanning]);

  const startScan = async () => {
    if (!session) return;
    setScanning(true);
    setError(null);
    try {
      await api.startScan(
        session.id,
        session.source_identity.path,
        session.source_size_bytes,
        session.sector_size,
      );
      const updated = await api.loadSession(session.id);
      setSession(updated);
      if (updated.status === "completed") {
        setProgress(100);
        setFilesFound(Number(updated.files_found));
        setCurrentStage("Complete");
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setScanning(false);
    }
  };

  const handlePause = async () => {
    if (!session) return;
    try {
      await api.pauseScan(session.id);
      setScanning(false);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleCancel = async () => {
    if (!session) return;
    try {
      await api.cancelScan(session.id);
      navigate("/");
    } catch (_) {}
  };

  const formatTime = (secs: number) => {
    if (secs < 60) return `${secs}s`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m ${s.toString().padStart(2, "0")}s`;
  };

  if (!session) {
    return (
      <div style={{ padding: "24px" }}>
        <div className="alert alert-warning">
          No active scan session.{" "}
          <button className="btn btn-ghost" onClick={() => navigate("/scan-setup")}>
            Go to Scan Setup
          </button>
        </div>
      </div>
    );
  }

  const isFinished = ["completed", "failed", "cancelled"].includes(session.status);
  const displayProgress = progress > 0 ? progress : (isFinished ? 100 : 0);

  const stageIndex = PIPELINE_STAGES.findIndex(
    (s) => currentStage && s.toLowerCase().includes(currentStage.toLowerCase().split(" ")[0])
  );

  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "24px" }}>
        <div>
          <h2>Scanning</h2>
          <p style={{ color: "var(--text-secondary)", marginTop: "4px", fontSize: "0.8125rem", fontFamily: "monospace" }}>
            {session.source_identity.path}
          </p>
        </div>
        <div style={{ display: "flex", gap: "8px" }}>
          {scanning && (
            <button className="btn btn-secondary" onClick={handlePause}>
              ⏸ Pause
            </button>
          )}
          {!isFinished && (
            <button className="btn btn-danger" onClick={handleCancel}>
              ✕ Cancel
            </button>
          )}
        </div>
      </div>

      {error && (
        <div className="alert alert-error" style={{ marginBottom: "16px" }}>{error}</div>
      )}

      {/* Status */}
      <div style={{ marginBottom: "16px" }}>
        <span className={`badge ${
          session.status === "running" ? "badge-green" :
          session.status === "completed" ? "badge-blue" :
          session.status === "failed" ? "badge-red" :
          session.status === "paused" ? "badge-amber" : "badge-gray"
        }`}>
          {session.status.toUpperCase()}
        </span>
        {currentStage && (
          <span style={{ marginLeft: "12px", color: "var(--text-secondary)", fontSize: "0.875rem" }}>
            {currentStage}
          </span>
        )}
      </div>

      {/* Progress bar */}
      <div className="card" style={{ marginBottom: "16px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "8px", fontSize: "0.875rem" }}>
          <span style={{ color: "var(--text-secondary)" }}>
            {formatBytes(processedBytes || Number(session.processed_bytes))} /
            {formatBytes(totalBytes || Number(session.source_size_bytes))}
          </span>
          <span style={{ color: "var(--text-secondary)" }}>{displayProgress.toFixed(1)}%</span>
        </div>
        <div className="progress-bar-track">
          <div className="progress-bar-fill" style={{ width: `${displayProgress}%` }} />
        </div>
        {speed > 0 && (
          <div style={{ marginTop: "6px", fontSize: "0.75rem", color: "var(--text-muted)" }}>
            {formatBytes(speed)}/s
            {eta != null && ` · ETA: ${formatTime(eta)}`}
          </div>
        )}
      </div>

      {/* Stats grid */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "10px", marginBottom: "16px" }}>
        {[
          { label: "Files Found", value: (filesFound || session.files_found).toLocaleString(), color: "var(--accent-green)" },
          { label: "Bad Sectors", value: (badSectors || session.bad_sectors).toLocaleString(), color: badSectors > 0 ? "var(--accent-red)" : "var(--text-primary)" },
          { label: "Elapsed", value: formatTime(elapsedSeconds), color: "var(--text-primary)" },
          { label: "Mode", value: session.configuration.mode.replace("_", " "), color: "var(--accent-blue)" },
        ].map((stat) => (
          <div key={stat.label} className="card" style={{ textAlign: "center", padding: "14px" }}>
            <div style={{ fontSize: "1.25rem", fontWeight: 700, color: stat.color }}>{stat.value}</div>
            <div style={{ fontSize: "0.7rem", color: "var(--text-muted)", marginTop: "4px", textTransform: "uppercase" }}>
              {stat.label}
            </div>
          </div>
        ))}
      </div>

      {/* Pipeline stages */}
      <div className="card">
        <h4 style={{ marginBottom: "14px" }}>Pipeline</h4>
        <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
          {PIPELINE_STAGES.map((stage, i) => {
            const isActive = scanning && currentStage &&
              stage.toLowerCase().includes(currentStage.toLowerCase().split(" ")[0]);
            const isDone = isFinished || (stageIndex > i);

            return (
              <div key={stage} style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                <div style={{
                  width: "20px", height: "20px",
                  borderRadius: "50%",
                  background: isDone ? "var(--accent-green)" : isActive ? "var(--accent-blue)" : "var(--bg-tertiary)",
                  display: "flex", alignItems: "center", justifyContent: "center",
                  fontSize: "0.65rem", flexShrink: 0, fontWeight: 700, color: "#fff",
                }}>
                  {isDone ? "✓" : i + 1}
                </div>
                <span style={{
                  fontSize: "0.875rem",
                  color: isDone ? "var(--text-primary)" : isActive ? "var(--accent-blue)" : "var(--text-muted)",
                  fontWeight: isActive ? 600 : 400,
                }}>
                  {stage}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Post-scan actions */}
      {isFinished && (
        <div style={{ marginTop: "20px", display: "flex", gap: "12px" }}>
          <button
            className="btn btn-primary btn-lg"
            onClick={() => navigate("/results", { state: { sessionId: session.id } })}
          >
            View Results ({(filesFound || session.files_found).toLocaleString()} files) →
          </button>
          <button className="btn btn-secondary" onClick={() => navigate("/sessions")}>
            All Sessions
          </button>
        </div>
      )}
    </div>
  );
}
