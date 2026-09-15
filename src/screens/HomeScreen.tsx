import { useNavigate } from "react-router-dom";

export function HomeScreen() {
  const navigate = useNavigate();

  const actions = [
    {
      icon: "💾",
      title: "Scan a Device",
      description: "Recover deleted files from internal drives, USB drives, SD cards, and more.",
      onClick: () => navigate("/devices"),
      accent: "var(--accent-blue)",
    },
    {
      icon: "📁",
      title: "Open Disk Image",
      description: "Analyse a RAW, DD, or IMG disk image file.",
      onClick: () => navigate("/scan-setup"),
      accent: "var(--accent-green)",
    },
    {
      icon: "🗂️",
      title: "Recent Sessions",
      description: "Resume a previous scan or view past recovery results.",
      onClick: () => navigate("/sessions"),
      accent: "var(--accent-purple)",
    },
  ];

  return (
    <div style={{
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      height: "100%",
      padding: "40px 24px",
      textAlign: "center",
    }}>
      {/* Hero */}
      <div style={{ marginBottom: "48px" }}>
        <div style={{ fontSize: "3.5rem", marginBottom: "16px" }}>◈</div>
        <h1 style={{ fontSize: "2.5rem", letterSpacing: "-0.03em", marginBottom: "12px" }}>
          RecoverX
        </h1>
        <p style={{ color: "var(--text-secondary)", fontSize: "1.125rem", maxWidth: "480px" }}>
          Recover deleted and lost files from internal drives, external drives, USB devices, and SD cards.
        </p>
      </div>

      {/* Action cards */}
      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(3, 1fr)",
        gap: "20px",
        maxWidth: "800px",
        width: "100%",
      }}>
        {actions.map((action) => (
          <button
            key={action.title}
            onClick={action.onClick}
            style={{
              background: "var(--bg-secondary)",
              border: `1px solid var(--border)`,
              borderRadius: "12px",
              padding: "28px 20px",
              cursor: "pointer",
              textAlign: "center",
              transition: "all 0.15s",
              color: "var(--text-primary)",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.borderColor = action.accent;
              (e.currentTarget as HTMLButtonElement).style.background = "var(--bg-tertiary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.borderColor = "var(--border)";
              (e.currentTarget as HTMLButtonElement).style.background = "var(--bg-secondary)";
            }}
          >
            <div style={{ fontSize: "2rem", marginBottom: "12px" }}>{action.icon}</div>
            <h3 style={{ marginBottom: "8px" }}>{action.title}</h3>
            <p style={{ fontSize: "0.8125rem", color: "var(--text-secondary)", lineHeight: 1.5 }}>
              {action.description}
            </p>
          </button>
        ))}
      </div>

      {/* SSD limitation notice */}
      <div className="alert alert-warning" style={{ marginTop: "40px", maxWidth: "600px", textAlign: "left" }}>
        <strong>Note:</strong> Recovery success depends on storage technology. SSDs with TRIM enabled
        may have limited recoverability. HDDs, USB drives, and SD cards generally retain deleted data
        until overwritten. RecoverX never modifies the source.
      </div>
    </div>
  );
}
