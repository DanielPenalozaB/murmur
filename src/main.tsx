import { QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { queryClient } from "./lib/queryClient";
import "./styles.css";

async function start() {
  // Dev-only design preview in a plain browser (see src/dev/preview.ts).
  if (import.meta.env.DEV && new URLSearchParams(location.search).has("preview")) {
    (await import("./dev/preview")).install();
  }
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </React.StrictMode>,
  );
}

void start();
