import { useQuery } from "@tanstack/react-query";
import { RecorderBar } from "./features/recorder/RecorderBar";
import { useRecorder } from "./features/recorder/useRecorder";
import { TranscriptView } from "./features/transcript/TranscriptView";
import { ipc } from "./lib/ipc";

export default function App() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: ipc.appInfo });
  const { status } = useRecorder();

  return (
    <div className="grid h-screen grid-cols-[260px_1fr]">
      {/* Leave room for the macOS traffic lights (titleBarStyle: Overlay). */}
      <aside
        className="flex flex-col border-r border-line bg-sidebar px-3 pt-12"
        data-tauri-drag-region
      >
        <h1 className="font-serif text-xl">Murmur</h1>
        <p className="mt-6 text-xs uppercase tracking-wide text-ink-soft">Folders</p>
        {info.data && (
          <p className="mt-auto pb-4 text-xs text-ink-soft">
            v{info.data.version} ·{" "}
            <span className={info.data.local ? "text-local" : "text-accent"}>{info.data.llm}</span>
          </p>
        )}
      </aside>
      <main className="flex min-h-0 flex-col">
        <RecorderBar />
        <TranscriptView recording={status !== null} />
      </main>
    </div>
  );
}
