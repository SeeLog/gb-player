import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import SettingsWindow from "./lib/Settings";
import ThemeEditorWindow from "./lib/ThemeEditor";
import "./app.css";

// The theme editor's and the settings windows load the same page (see app/src/gui/side.rs).
const Root =
  { "theme-editor": ThemeEditorWindow, settings: SettingsWindow }[getCurrentWindow().label] ?? App;

createRoot(document.getElementById("app")!).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
