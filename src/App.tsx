import { useState } from "react";
import { LivePanel } from "./features/assistant/LivePanel";
import { FolderView } from "./features/library/FolderView";
import { SearchView } from "./features/library/SearchView";
import { SessionView } from "./features/library/SessionView";
import { Sidebar } from "./features/library/Sidebar";
import { RecorderBar } from "./features/recorder/RecorderBar";
import { useRecorder } from "./features/recorder/useRecorder";
import type { Seek } from "./features/transcript/TranscriptTurns";
import { TranscriptView } from "./features/transcript/TranscriptView";
import { useNav } from "./lib/nav";

export default function App() {
  const view = useNav((s) => s.view);
  const { status } = useRecorder();
  const recording = status !== null;
  const [seek, setSeek] = useState<Seek | null>(null);

  return (
    <div className="grid h-screen grid-cols-[260px_1fr]">
      <Sidebar recording={recording} />
      <main className="flex min-h-0 flex-col">
        {view.kind === "live" && (
          <div className="grid min-h-0 flex-1 grid-cols-[1fr_340px]">
            <div className="flex min-h-0 flex-col">
              <RecorderBar />
              <TranscriptView recording={recording} seek={seek} />
            </div>
            <LivePanel onSeek={(ms) => setSeek({ ms, nonce: Date.now() })} />
          </div>
        )}
        {view.kind === "folder" && <FolderView folder={view.folder} />}
        {view.kind === "session" && <SessionView id={view.id} />}
        {view.kind === "search" && <SearchView query={view.query} />}
      </main>
    </div>
  );
}
