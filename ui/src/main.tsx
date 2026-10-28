import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import ThemeEditorWindow from "./lib/ThemeEditor";
import "./app.css";

// The theme editor's window loads the same page (see app/src/gui/editor.rs).
const Root = getCurrentWindow().label === "theme-editor" ? ThemeEditorWindow : App;

createRoot(document.getElementById("app")!).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
