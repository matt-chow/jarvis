import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { applyLaunchRoute } from "./lib/launch";
import "./index.css";

// Before the first render: the installed app may open on Ask (lib/launch.ts).
applyLaunchRoute();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
