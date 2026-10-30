import { useQuery } from "@tanstack/react-query";
import { FolderView } from "./features/library/FolderView";
import { SearchView } from "./features/library/SearchView";
import { SessionView } from "./features/library/SessionView";
import { Sidebar } from "./features/library/Sidebar";
import { useVaultSync } from "./features/library/useVaultSync";
import { LiveView } from "./features/recorder/LiveView";
import { MeetingBanner } from "./features/recorder/MeetingBanner";
import { useRecorder } from "./features/recorder/useRecorder";
import { useTrayActions } from "./features/recorder/useTrayActions";
import { SettingsView } from "./features/settings/SettingsView";
import { ModelSetup } from "./features/setup/ModelSetup";
import { SpeakersView } from "./features/speakers/SpeakersView";
import { ipc } from "./lib/ipc";
import { useNav } from "./lib/nav";

export default function App() {
  const view = useNav((s) => s.view);
  const { status } = useRecorder();
  useTrayActions();
  useVaultSync();
  const missing = useQuery({ queryKey: ["missing-models"], queryFn: ipc.missingModels });
  // Recording needs the models; the library, notes and settings work without them.
  const setup = (missing.data?.length ?? 0) > 0 && view.kind === "live";

  return (
    <div className="grid h-screen grid-cols-[264px_1fr]">
      <Sidebar recording={status !== null} />
      <main className="flex min-h-0 min-w-0 flex-col">
        <MeetingBanner recording={status !== null} />
        {setup && <ModelSetup missing={missing.data ?? []} />}
        {view.kind === "live" && !setup && <LiveView />}
        {view.kind === "folder" && <FolderView key={String(view.folder)} folder={view.folder} />}
        {view.kind === "session" && <SessionView key={view.id} id={view.id} />}
        {view.kind === "search" && <SearchView key={view.query} query={view.query} />}
        {view.kind === "speakers" && <SpeakersView />}
        {view.kind === "settings" && <SettingsView />}
      </main>
    </div>
  );
}
