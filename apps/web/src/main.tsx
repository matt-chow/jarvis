import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { applyLaunchRoute } from "./lib/launch";
import { applyTheme } from "./lib/theme";
import "./index.css";

// Before the first render: the installed app may open on Ask (lib/launch.ts),
// and this device's Light / Night choice applies without a flash.
applyLaunchRoute();
applyTheme();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
