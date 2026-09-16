import { useEffect, useState } from "react";
import ControlPanel from "./components/ControlPanel";
import ToolOverlay from "./components/ToolOverlay";
import VirtualScreen from "./components/VirtualScreen";
import "./control-panel.css";
import "./virtual-screen.css";

const params = new URLSearchParams(window.location.search);
const IS_VIRTUAL_SCREEN = params.has("virtual-screen");
const IS_TOOL_OVERLAY = params.has("tool-overlay");

export default function App() {
  const [ready, setReady] = useState(false);

  useEffect(() => {
    setReady(true);
  }, []);

  if (!ready) return null;

  if (IS_VIRTUAL_SCREEN) {
    return <VirtualScreen />;
  }

  if (IS_TOOL_OVERLAY) {
    return <ToolOverlay />;
  }

  return <ControlPanel />;
}
