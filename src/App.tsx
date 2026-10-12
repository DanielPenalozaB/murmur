import { useQuery } from "@tanstack/react-query";
import { RecorderPanel } from "./features/recorder/RecorderPanel";
import { ipc } from "./lib/ipc";

export default function App() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: ipc.appInfo });

  return (
    <div className="grid h-screen grid-cols-[260px_1fr]">
      {/* Leave room for the macOS traffic lights (titleBarStyle: Overlay). */}
      <aside className="border-r border-line bg-sidebar px-3 pt-12" data-tauri-drag-region>
        <h1 className="font-serif text-xl">Murmur</h1>
        <p className="mt-6 text-xs uppercase tracking-wide text-ink-soft">Folders</p>
      </aside>
      <main className="flex flex-col items-center justify-center gap-10">
        <RecorderPanel />
        {info.data && (
          <p className="text-xs text-ink-soft">
            v{info.data.version} ·{" "}
            <span className={info.data.local ? "text-local" : "text-accent"}>{info.data.llm}</span>
          </p>
        )}
      </main>
    </div>
  );
}
