import { QueryClientProvider } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { MomentCard } from "./features/moments/MomentCard";
import { QuickNote } from "./features/moments/QuickNote";
import { MeetingPopup } from "./features/recorder/MeetingPopup";
import { queryClient } from "./lib/queryClient";
import "./styles.css";

/** The small windows over other apps, by label: the meeting offer, the moment card and
 * the quick-note box. */
const CARDS = ["meeting", "moment", "note"] as const;
type Card = (typeof CARDS)[number];

function cardWindow(): Card | null {
  try {
    const label = getCurrentWindow().label;
    return CARDS.find((c) => c === label) ?? null;
  } catch {
    return null;
  }
}

/** The preview names them `view=popup…`, `view=moment…` and `view=note`. */
function previewCard(view: string): Card | null {
  if (view.startsWith("popup")) return "meeting";
  if (view.startsWith("moment")) return "moment";
  if (view === "note") return "note";
  return null;
}

async function start() {
  const params = new URLSearchParams(location.search);
  // Dev-only design preview in a plain browser (see src/dev/preview.ts).
  if (import.meta.env.DEV && params.has("preview")) {
    (await import("./dev/preview")).install();
  }
  const card = cardWindow() ?? (import.meta.env.DEV ? previewCard(params.get("view") ?? "") : null);
  // A boolean: classList.toggle with `undefined` flips the class instead of removing it.
  document.documentElement.classList.toggle("popup", card !== null);
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        {card === "meeting" ? (
          <MeetingPopup initiallyCollapsed={params.get("view") === "popup-collapsed"} />
        ) : card === "moment" ? (
          <MomentCard />
        ) : card === "note" ? (
          <QuickNote />
        ) : (
          <App />
        )}
      </QueryClientProvider>
    </React.StrictMode>,
  );
}

void start();
