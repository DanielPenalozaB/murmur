import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AudioLines, CalendarDays, ChevronDown, ShieldCheck, Trash2 } from "lucide-react";
import { useState } from "react";
import { Avatar, PageHeader, TopBar } from "../../components/ui";
import { formatRelative, ipc, type VoiceEntry } from "../../lib/ipc";
import { useNav } from "../../lib/nav";

function VoiceCard({ voice }: { voice: VoiceEntry }) {
  const go = useNav((s) => s.go);
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  // Browser dialogs (window.confirm) don't show in the app's web view, so confirm inline.
  const [confirming, setConfirming] = useState(false);
  const sessions = useQuery({
    queryKey: ["voice-sessions", voice.name],
    queryFn: () => ipc.sessionsWithSpeaker(voice.name),
    enabled: open,
  });
  const forget = useMutation({
    mutationFn: () => ipc.forgetVoice(voice.name),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["voices"] }),
  });

  return (
    <div className="flex flex-col gap-3 rounded-xl bg-muted/60 p-4 hover:bg-muted">
      <div className="flex items-start gap-3">
        <Avatar name={voice.name} size="lg" />
        <div className="min-w-0 flex-1">
          <p className="truncate font-bold">{voice.name}</p>
          <p className="text-[13px] text-ink-soft">
            {voice.last_seen ? `Last heard ${formatRelative(voice.last_seen)}` : "Not heard yet"}
          </p>
        </div>
        <span className="chip bg-local-soft text-on-local-soft">Voiceprint</span>
      </div>
      <div className="grid grid-cols-2 gap-2 text-[13px]">
        <div className="flex items-center gap-1.5 rounded bg-surface/80 p-1.5">
          <CalendarDays className="size-4 text-outline" />
          <span className="font-medium">{voice.meetings}</span>
          <span className="text-[11px] text-outline">
            {voice.meetings === 1 ? "meeting" : "meetings"}
          </span>
        </div>
        <div className="flex items-center gap-1.5 rounded bg-surface/80 p-1.5">
          <AudioLines className="size-4 text-outline" />
          <span className="text-[11px] text-outline">Recognized automatically</span>
        </div>
      </div>
      <div className="flex items-center justify-between">
        {confirming ? (
          <span className="flex items-center gap-2 text-[13px]">
            <span className="text-ink-soft">Forget {voice.name}? Past notes keep the name.</span>
            <button type="button" className="btn-quiet text-accent" onClick={() => forget.mutate()}>
              Forget
            </button>
            <button type="button" className="btn-quiet" onClick={() => setConfirming(false)}>
              Cancel
            </button>
          </span>
        ) : (
          <button
            type="button"
            className="btn-quiet text-accent"
            onClick={() => setConfirming(true)}
          >
            <Trash2 className="size-3.5" /> Forget voice
          </button>
        )}
        <button type="button" className="btn-ghost py-1" onClick={() => setOpen(!open)}>
          Meetings{" "}
          <ChevronDown className={`size-3.5 transition-transform ${open ? "rotate-180" : ""}`} />
        </button>
      </div>
      {open && (
        <div className="flex flex-col gap-1">
          {sessions.data?.length === 0 && (
            <p className="text-[13px] text-ink-soft">No meetings yet.</p>
          )}
          {sessions.data?.map((s) => (
            <button
              key={s.id}
              type="button"
              className="flex items-center justify-between rounded-lg bg-surface px-3 py-2 text-left text-[14px] hover:bg-high"
              onClick={() => go({ kind: "session", id: s.id })}
            >
              <span className="truncate font-serif text-[16px]">{s.title}</span>
              <span className="shrink-0 text-[12px] text-ink-soft">
                {formatRelative(s.started_at)}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function SpeakersView() {
  const voices = useQuery({ queryKey: ["voices"], queryFn: ipc.listVoices });
  const count = voices.data?.length ?? 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar crumbs={["Vault", "Speakers"]} />
      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
        <div className="mx-auto flex max-w-[1180px] flex-col gap-8 pt-2">
          <PageHeader
            eyebrow={
              <>
                <span>Intelligence</span>
                <span className="text-outline">/</span>
                <span className="text-accent">Speaker registry</span>
              </>
            }
            title="Speaker Directory"
            subtitle="People Murmur recognizes by voice. Name someone once and they're labelled in every meeting after."
          >
            <span className="chip bg-local-soft py-1 text-on-local-soft">{count} active</span>
          </PageHeader>

          <div className="grid grid-cols-[1fr_340px] items-start gap-6">
            <div className="grid grid-cols-2 gap-3">
              {voices.data?.map((v) => (
                <VoiceCard key={v.name} voice={v} />
              ))}
              {count === 0 && voices.isSuccess && (
                <div className="card col-span-2 p-8 text-center text-ink-soft">
                  No voices yet. After a meeting, open its notes and click “Speaker 1” to name them.
                </div>
              )}
            </div>
            <aside className="card flex flex-col gap-4 p-5 text-[14px]">
              <h3 className="font-serif text-xl">How speakers are found</h3>
              <ol className="flex list-decimal flex-col gap-2 pl-5 text-ink-soft marker:text-accent">
                <li>When a meeting ends, the call audio is split into speakers by voice.</li>
                <li>Each person gets a voiceprint: a short numeric summary of how they sound.</li>
                <li>When you name someone, their voiceprint is saved under that name.</li>
                <li>In later meetings, matching voices get the name automatically.</li>
              </ol>
              <div className="flex gap-2 rounded-lg bg-sidebar p-3 text-[13px] text-ink-soft">
                <ShieldCheck className="size-4 shrink-0 text-local" />
                Voiceprints and audio stay in your local database. Nothing is sent anywhere.
              </div>
            </aside>
          </div>
        </div>
      </div>
    </div>
  );
}
