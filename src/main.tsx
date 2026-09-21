import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import MainApp from "./main/MainApp";
import Overlay from "./overlay/Overlay";

// Both windows load this page; the window label decides which view renders.
const label = getCurrentWindow().label;
document.documentElement.dataset.window = label;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{label === "overlay" ? <Overlay /> : <MainApp />}</React.StrictMode>,
);
