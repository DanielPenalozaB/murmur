import { AtSign, Bookmark, X } from "lucide-react";
import { useEffect, useState } from "react";
import { LogoMark } from "../../components/ui";
import { ipc, type Moment } from "../../lib/ipc";
import { formatTimestamp } from "../transcript/group";

/** How long the card stays: long enough to read a mention, a glance for a mark. */
const SHOWN_MS = { mention: 15_000, marked: 2_500 };

/** The card over other apps: someone said the user's name, or a moment was marked. */
export function MomentCard() {
  const [moment, setMoment] = useState<Moment | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = ipc.onMomentCard((m) => active && setMoment(m));
    ipc.momentCard().then((m) => active && setMoment(m));
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  // Each new moment restarts the timer.
  useEffect(() => {
    if (!moment) return;
    const ms = moment.kind === "marked" ? SHOWN_MS.marked : SHOWN_MS.mention;
    const timer = setTimeout(() => void ipc.dismissMomentCard(), ms);
    return () => clearTimeout(timer);
  }, [moment]);

  if (!moment) return null;
  const at = formatTimestamp(moment.at);

  if (moment.kind === "marked") {
    return (
      // The window is sized to this (MARK_SIZE in src-tauri/src/moments.rs).
      <div className="flex h-screen items-center gap-2.5 rounded-xl bg-surface px-3 text-ink ring-1 ring-line ring-inset">
        <span className="rounded-lg bg-accent-soft p-1.5 text-on-accent-soft">
          <Bookmark className="size-4" />
        </span>
        <p className="text-[13px] font-semibold">
          Moment marked <span className="font-normal text-ink-soft">at {at}</span>
        </p>
      </div>
    );
  }

  return (
    // The window is sized to this card (see CARD_SIZE in src-tauri/src/moments.rs).
    <div className="flex h-screen items-center gap-3 rounded-xl bg-surface pr-2 pl-3 text-ink ring-1 ring-line ring-inset">
      <LogoMark size="sm" />
      <button
        type="button"
        className="min-w-0 flex-1 text-left leading-tight"
        title="Show it in the live transcript"
        onClick={() => void ipc.openMoment()}
      >
        <p className="flex items-center gap-1.5 text-[13px] font-semibold">
          <AtSign className="size-3.5 text-accent" />
          {moment.kind === "asked" ? "You were asked" : "You were mentioned"}
          <span className="font-normal text-ink-soft">· {at}</span>
        </p>
        <p className="mt-0.5 line-clamp-2 text-[12px] text-ink-soft">“{moment.text}”</p>
      </button>
      <button
        type="button"
        className="self-start rounded-md p-1 text-ink-soft hover:bg-muted hover:text-ink mt-2"
        aria-label="Dismiss"
        onClick={() => void ipc.dismissMomentCard()}
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
