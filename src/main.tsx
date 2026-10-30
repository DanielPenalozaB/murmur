import { QueryClientProvider } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { MeetingPopup } from "./features/recorder/MeetingPopup";
import { queryClient } from "./lib/queryClient";
import "./styles.css";

/** The floating card shown over other apps when a meeting is detected. */
function isPopup() {
  try {
    return getCurrentWindow().label === "meeting";
  } catch {
    return false;
  }
}

async function start() {
  const params = new URLSearchParams(location.search);
  // Dev-only design preview in a plain browser (see src/dev/preview.ts).
  if (import.meta.env.DEV && params.has("preview")) {
    (await import("./dev/preview")).install();
  }
  const popup = isPopup() || (import.meta.env.DEV && params.get("view") === "popup");
  document.documentElement.classList.toggle("popup", popup);
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        {popup ? <MeetingPopup /> : <App />}
      </QueryClientProvider>
    </React.StrictMode>,
  );
}

void start();
