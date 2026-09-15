export function SettingsScreen() {
  return (
    <div style={{ padding: "24px", height: "100%", overflow: "auto" }}>
      <h2 style={{ marginBottom: "24px" }}>Settings</h2>

      <div className="card" style={{ marginBottom: "16px" }}>
        <h3 style={{ marginBottom: "16px" }}>Source Protection</h3>
        <div className="alert alert-info">
          Source write protection is always active and cannot be disabled.
          All storage sources are opened read-only. The SourceWriteGuard blocks
          any attempt to write to a registered source device or image.
        </div>
      </div>

      <div className="card" style={{ marginBottom: "16px" }}>
        <h3 style={{ marginBottom: "16px" }}>Recovery Safety</h3>
        <div style={{ fontSize: "0.875rem", color: "var(--text-secondary)", lineHeight: 1.7 }}>
          <p>• The destination folder is always validated before recovery begins.</p>
          <p>• Recovery to the same device as the source is blocked.</p>
          <p>• All recovered files are written to temporary files then renamed (atomic writes).</p>
          <p>• SHA-256 hash is computed and displayed for every recovered file.</p>
          <p>• Filename sanitisation prevents path traversal in recovered filenames.</p>
        </div>
      </div>

      <div className="card" style={{ marginBottom: "16px" }}>
        <h3 style={{ marginBottom: "16px" }}>SSD and TRIM Limitations</h3>
        <div className="alert alert-warning">
          <strong>Important notice:</strong> Recovery success depends on the storage technology.
        </div>
        <div style={{ fontSize: "0.875rem", color: "var(--text-secondary)", lineHeight: 1.7, marginTop: "12px" }}>
          <p>
            <strong style={{ color: "var(--text-primary)" }}>SSDs with TRIM:</strong> Deleted data may be immediately
            erased by the drive firmware. Recovery is often limited or impossible.
          </p>
          <p style={{ marginTop: "8px" }}>
            <strong style={{ color: "var(--text-primary)" }}>HDDs:</strong> Deleted data remains on disk until
            overwritten. Recovery is generally possible until the space is reused.
          </p>
          <p style={{ marginTop: "8px" }}>
            <strong style={{ color: "var(--text-primary)" }}>USB drives and SD cards:</strong> Similar to HDDs —
            deleted data may be recoverable until overwritten.
          </p>
          <p style={{ marginTop: "8px" }}>
            <strong style={{ color: "var(--text-primary)" }}>Encrypted volumes (FileVault, BitLocker):</strong>
            Encrypted data cannot be recovered without the decryption key.
          </p>
        </div>
      </div>

      <div className="card">
        <h3 style={{ marginBottom: "16px" }}>About</h3>
        <div style={{ display: "flex", flexDirection: "column", gap: "8px", fontSize: "0.875rem" }}>
          {[
            ["Application", "RecoverX"],
            ["Version", "0.1.0"],
            ["Purpose", "Data Recovery Tool"],
            ["Backend", "Rust + Tauri 2"],
            ["Frontend", "React 19 + TypeScript"],
            ["Database", "SQLite (WAL mode)"],
            ["Source", "Always read-only"],
          ].map(([key, val]) => (
            <div key={key} style={{ display: "flex", gap: "12px" }}>
              <span style={{ color: "var(--text-secondary)", width: "140px", flexShrink: 0 }}>{key}</span>
              <span>{val}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
