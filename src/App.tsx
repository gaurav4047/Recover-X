import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Sidebar } from "./components/Sidebar";
import { HomeScreen } from "./screens/HomeScreen";
import { DevicesScreen } from "./screens/DevicesScreen";
import { ScanSetupScreen } from "./screens/ScanSetupScreen";
import { ScanningScreen } from "./screens/ScanningScreen";
import { ResultsScreen } from "./screens/ResultsScreen";
import { RecoveryScreen } from "./screens/RecoveryScreen";
import { SessionsScreen } from "./screens/SessionsScreen";
import "./styles.css";

export default function App() {
  return (
    <BrowserRouter>
      <div style={{
        display: "flex",
        height: "100vh",
        overflow: "hidden",
        background: "var(--bg-primary)",
      }}>
        <Sidebar />
        <main style={{
          flex: 1,
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}>
          <Routes>
            <Route path="/" element={<HomeScreen />} />
            <Route path="/devices" element={<DevicesScreen />} />
            <Route path="/scan-setup" element={<ScanSetupScreen />} />
            <Route path="/scanning" element={<ScanningScreen />} />
            <Route path="/results" element={<ResultsScreen />} />
            <Route path="/recovery" element={<RecoveryScreen />} />
            <Route path="/sessions" element={<SessionsScreen />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}
