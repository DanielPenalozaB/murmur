import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowUpRight, Check, CircleAlert, UserX } from "lucide-react";
import { useState } from "react";
import { formatRelative, ipc, type ReviewItem, type ReviewSpeaker } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { SpeakerChip } from "../transcript/TranscriptTurns";
import { describe, percent, refreshSpeakers } from "./review";

export function useSpeakerReview() {
  return useQuery({ queryKey: ["speaker-review"], queryFn: ipc.speakerReview });
}

/** One named speaker in a finding: fix the name, take it away, or go listen. */
function SpeakerRow({
  name,
  speaker,
  score,
}: {
  name: string;
  speaker: ReviewSpeaker;
  /** How alike this one is to the speaker it's compared with. */
  score?: number;
}) {
  const go = useNav((s) => s.go);
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const done = () => refreshSpeakers(queryClient, speaker.session_id);
  const unname = useMutation({
    mutationFn: () => ipc.unnameSpeaker(speaker.session_id, speaker.speaker_id),
    onSuccess: done,
    onError: (e) => setError(String(e)),
  });

  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg bg-surface px-3 py-2 text-[13px]">
      <SpeakerChip
        label={name}
        channel="system"
        speakerId={speaker.speaker_id}
        onRename={async (speakerId, newName) => {
          await ipc.renameSpeaker(speaker.session_id, speakerId, newName);
          await done();
        }}
      />
      <span className="min-w-0 flex-1 truncate text-ink-soft">
        {speaker.label} · {speaker.title} · {formatRelative(speaker.started_at)}
      </span>
      {score !== undefined && (
        <span className="shrink-0 tabular-nums text-ink-soft">{percent(score)} alike</span>
      )}
      <button
        type="button"
        className="btn-quiet"
        title={`Make them ${speaker.label} again and take their voice out of ${name}'s`}
        disabled={unname.isPending}
        onClick={() => unname.mutate()}
      >
        <UserX className="size-3.5" /> Remove name
      </button>
      <button
        type="button"
        className="btn-quiet"
        title="Open the meeting to listen"
        onClick={() => go({ kind: "session", id: speaker.session_id })}
      >
        Open <ArrowUpRight className="size-3.5" />
      </button>
      {error && <span className="w-full text-[12px] text-accent">{error}</span>}
    </div>
  );
}

function Finding({ item }: { item: ReviewItem }) {
  const queryClient = useQueryClient();
  const dismiss = useMutation({
    mutationFn: () => ipc.dismissReview(item.key),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["speaker-review"] }),
  });
  const { title, detail } = describe(item);

  return (
    <div className="flex flex-col gap-3 rounded-xl bg-muted/60 p-4">
      <div className="flex items-start gap-3">
        <CircleAlert className="mt-0.5 size-4 shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <p className="font-bold">{title}</p>
          <p className="text-[13px] text-ink-soft">{detail}</p>
        </div>
        <button
          type="button"
          className="btn-ghost shrink-0 py-1"
          title="The names are right; stop showing this"
          disabled={dismiss.isPending}
          onClick={() => dismiss.mutate()}
        >
          <Check className="size-3.5" /> Looks right
        </button>
      </div>
      {item.kind === "pair" &&
        item.speakers.map((s) => (
          <SpeakerRow key={s.speaker_id + s.session_id} name={item.name} speaker={s} />
        ))}
      {item.kind === "outlier" && (
        <>
          <SpeakerRow name={item.name} speaker={item.speaker} />
          <p className="eyebrow">Compared with {item.name} in</p>
          {item.compared.map((c) => (
            <SpeakerRow
              key={c.speaker.speaker_id + c.speaker.session_id}
              name={item.name}
              speaker={c.speaker}
              score={c.score}
            />
          ))}
        </>
      )}
    </div>
  );
}

/** Named speakers that may have the wrong name, worst first. Hidden when all is well. */
export function SpeakerReview() {
  const review = useSpeakerReview();
  const items = review.data ?? [];
  if (items.length === 0) return null;
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-baseline justify-between gap-4">
        <h2 className="font-serif text-2xl">Needs review</h2>
        <p className="text-[13px] text-ink-soft">
          Click a name to correct it. Murmur rebuilds the remembered voice from the speakers left
          under each name.
        </p>
      </div>
      {items.map((item) => (
        <Finding key={item.key} item={item} />
      ))}
    </section>
  );
}
