import { NavLink } from "react-router-dom";

const navItems = [
  { to: "/", label: "Home", icon: "⬡", exact: true },
  { to: "/devices", label: "Devices", icon: "💾" },
  { to: "/scan-setup", label: "Scan", icon: "🔍" },
  { to: "/results", label: "Results", icon: "📋" },
  { to: "/recovery", label: "Recovery", icon: "↗️" },
  { to: "/sessions", label: "Sessions", icon: "🗂️" },
];

export function Sidebar() {
  return (
    <aside style={{
      width: "var(--sidebar-width)",
      height: "100%",
      background: "var(--bg-secondary)",
      borderRight: "1px solid var(--border)",
      display: "flex",
      flexDirection: "column",
      flexShrink: 0,
    }}>
      {/* Logo */}
      <div style={{
        padding: "20px 16px",
        borderBottom: "1px solid var(--border)",
      }}>
        <div style={{
          fontSize: "1.25rem",
          fontWeight: 800,
          color: "var(--text-primary)",
          letterSpacing: "-0.02em",
        }}>
          ◈ RecoverX
        </div>
        <div style={{
          fontSize: "0.7rem",
          color: "var(--text-muted)",
          marginTop: "2px",
          textTransform: "uppercase",
          letterSpacing: "0.08em",
        }}>
          Data Recovery
        </div>
      </div>

      {/* Nav items */}
      <nav style={{ padding: "12px 8px", flex: 1 }}>
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.exact}
            style={({ isActive }) => ({
              display: "flex",
              alignItems: "center",
              gap: "10px",
              padding: "9px 10px",
              borderRadius: "6px",
              textDecoration: "none",
              fontSize: "0.875rem",
              fontWeight: 500,
              color: isActive ? "var(--text-primary)" : "var(--text-secondary)",
              background: isActive ? "var(--bg-tertiary)" : "transparent",
              marginBottom: "2px",
              transition: "all 0.1s",
            })}
          >
            <span style={{ fontSize: "1rem" }}>{item.icon}</span>
            {item.label}
          </NavLink>
        ))}
      </nav>

      {/* Footer */}
      <div style={{
        padding: "12px 16px",
        borderTop: "1px solid var(--border)",
        fontSize: "0.75rem",
        color: "var(--text-muted)",
      }}>
        v0.1.0
      </div>
    </aside>
  );
}
