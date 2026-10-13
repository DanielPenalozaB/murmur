import { FolderView } from "./features/library/FolderView";
import { SearchView } from "./features/library/SearchView";
import { SessionView } from "./features/library/SessionView";
import { Sidebar } from "./features/library/Sidebar";
import { LiveView } from "./features/recorder/LiveView";
import { useRecorder } from "./features/recorder/useRecorder";
import { SettingsView } from "./features/settings/SettingsView";
import { SpeakersView } from "./features/speakers/SpeakersView";
import { useNav } from "./lib/nav";

export default function App() {
  const view = useNav((s) => s.view);
  const { status } = useRecorder();

  return (
    <div className="grid h-screen grid-cols-[264px_1fr]">
      <Sidebar recording={status !== null} />
      <main className="flex min-h-0 min-w-0 flex-col">
        {view.kind === "live" && <LiveView />}
        {view.kind === "folder" && <FolderView key={String(view.folder)} folder={view.folder} />}
        {view.kind === "session" && <SessionView key={view.id} id={view.id} />}
        {view.kind === "search" && <SearchView key={view.query} query={view.query} />}
        {view.kind === "speakers" && <SpeakersView />}
        {view.kind === "settings" && <SettingsView />}
      </main>
    </div>
  );
}
