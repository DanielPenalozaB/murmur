import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowUpRight, Check, CircleCheck, Play } from "lucide-react";
import { useMemo, useState } from "react";
import { Avatar, PageHeader, TopBar } from "../../components/ui";
import { formatRelative, ipc } from "../../lib/ipc";
import { folderCrumbs, useNav } from "../../lib/nav";
import { AudioPlayer } from "../player/AudioPlayer";
import { usePlayer } from "../player/player";
import { formatTimestamp, groupIntoTurns, speakerLabel, turnAt } from "../transcript/group";
import { collectTasks, dueLabel, type Group, groupOf, type Task } from "./actions";

export function useTasks() {
  const settings = useQuery({ queryKey: ["settings"], queryFn: ipc.getSettings });
  const notes = useQuery({ queryKey: ["tasks"], queryFn: ipc.taskNotes });
  const userName = settings.data?.user_name ?? "";
  const tasks = useMemo(() => collectTasks(notes.data ?? [], userName), [notes.data, userName]);
  return { tasks, userName, error: notes.error };
}

/** Ticks a task in its meeting's notes and refreshes everything that shows it. */
function useToggle() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (task: Task) => ipc.setActionDone(task.sessionId, task.line, !task.done),
    onSettled: (_data, _error, task) =>
      Promise.all(
        [["tasks"], ["sessions"], ["session", task.sessionId]].map((queryKey) =>
          queryClient.invalidateQueries({ queryKey }),
        ),
      ),
  });
}

const GROUPS: { id: Group; title: string; hint: string }[] = [
  { id: "overdue", title: "Overdue", hint: "Past the date that was mentioned" },
  { id: "week", title: "Due this week", hint: "In the next 7 days" },
  { id: "later", title: "Later", hint: "More than a week away" },
  { id: "undated", title: "No due date", hint: "Newest meetings first" },
];

type Filter = "mine" | "open" | "done";

const sentence = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

function Stat({ label, value, tone }: { label: string; value: number; tone?: "alert" }) {
  return (
    <div className="card flex flex-col gap-1 p-4">
      <span className="eyebrow">{label}</span>
      <span
        className={`font-serif text-[32px] leading-none tabular-nums ${tone === "alert" && value > 0 ? "text-accent" : ""}`}
      >
        {value}
      </span>
    </div>
  );
}

function DueChip({ task, today }: { task: Task; today: Date }) {
  if (!task.due) return null;
  const late = !task.done && groupOf(task, today) === "overdue";
  return (
    <span
      className={`chip ${late ? "bg-accent-soft text-on-accent-soft" : "bg-muted text-ink-soft"}`}
    >
      {late ? "Overdue · " : "Due "}
      {dueLabel(task.due, today)}
    </span>
  );
}

function Owners({ task }: { task: Task }) {
  if (task.owners.length === 0) return <span className="text-ink-soft">No owner</span>;
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      {task.owners.slice(0, 3).map((o) => (
        <span key={o} className="flex items-center gap-1">
          <Avatar name={o} size="xs" /> <span className="truncate">{o}</span>
        </span>
      ))}
      {task.owners.length > 3 && <span>+{task.owners.length - 3}</span>}
    </span>
  );
}

function TaskRow({
  task,
  today,
  selected,
  onSelect,
}: {
  task: Task;
  today: Date;
  selected: boolean;
  onSelect: () => void;
}) {
  const toggle = useToggle();
  const done = toggle.isPending ? !task.done : task.done;
  return (
    <div
      className={`flex items-start gap-3 rounded-xl px-4 py-3 ${selected ? "bg-surface ring-1 ring-accent/30" : "bg-muted/60 hover:bg-muted"}`}
    >
      <button
        type="button"
        className={`mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-md ring-1 ring-inset ${done ? "bg-local text-canvas ring-local" : "ring-outline hover:ring-accent"}`}
        aria-label={done ? "Mark as not done" : "Mark as done"}
        title={done ? "Mark as not done" : "Mark as done (updates the meeting's notes)"}
        disabled={toggle.isPending}
        onClick={() => toggle.mutate(task)}
      >
        {done && <Check className="size-3.5" />}
      </button>
      <button type="button" className="min-w-0 flex-1 text-left" onClick={onSelect}>
        <p className={`text-[15px] ${done ? "text-ink-soft line-through" : ""}`}>
          {sentence(task.text)}
        </p>
        <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-ink-soft">
          <Owners task={task} />
          <DueChip task={task} today={today} />
          <span className="truncate">
            {task.title} · {formatRelative(task.startedAt)}
          </span>
        </div>
      </button>
      {toggle.error && <span className="text-[12px] text-accent">{String(toggle.error)}</span>}
    </div>
  );
}

/** What was said when the task was agreed, with the recording to hear it. */
function TaskDetail({ task, today }: { task: Task; today: Date }) {
  const go = useNav((s) => s.go);
  const toggle = useToggle();
  const session = useQuery({
    queryKey: ["session", task.sessionId],
    queryFn: () => ipc.getSession(task.sessionId),
  });
  const s = session.data;
  const turns = useMemo(() => (s ? groupIntoTurns(s.segments) : []), [s]);
  // The turn the citation points at, and the one after for the answer.
  // A citation past the end of the transcript was made up; show nothing for it.
  const end = Math.max(0, ...(s?.segments.map((seg) => seg.end) ?? []));
  const at =
    task.at === null || task.at > end + 5000 ? -1 : Math.max(0, turnAt(turns, task.at + 999));
  const excerpt = at === -1 ? [] : turns.slice(at, at + 2);
  // A few seconds early, to hear the lead-in.
  const from = Math.max(0, (task.at ?? 0) - 3000);

  return (
    <div className="card flex min-h-0 flex-col gap-4 overflow-y-auto p-5">
      <span className="eyebrow">Action item</span>
      <p
        className={`font-serif text-2xl leading-snug ${task.done ? "text-ink-soft line-through" : ""}`}
      >
        {sentence(task.text)}
      </p>
      <div className="flex flex-wrap items-center gap-2 text-[13px]">
        <Owners task={task} />
        <DueChip task={task} today={today} />
      </div>

      {excerpt.length > 0 && s && (
        <div className="flex flex-col gap-3 rounded-xl bg-sidebar p-4">
          <span className="eyebrow">Said at {formatTimestamp(task.at ?? 0)}</span>
          {excerpt.map((turn) => (
            <div key={turn.start} className="flex flex-col gap-1">
              <span className="text-[12px] font-semibold text-ink-soft">
                {speakerLabel(turn.channel, turn.speaker, s.speakers)} ·{" "}
                {formatTimestamp(turn.start)}
              </span>
              <p className="line-clamp-4 font-serif text-[16px] leading-relaxed italic">
                “{turn.texts.join(" ")}”
              </p>
            </div>
          ))}
        </div>
      )}

      {task.hasAudio && task.at !== null && s && (
        <div className="flex flex-col gap-2">
          <button
            type="button"
            className="btn-ghost self-start"
            onClick={() => usePlayer.getState().play(from)}
          >
            <Play className="size-3.5 fill-current" /> Hear the moment
          </button>
          <AudioPlayer sessionId={task.sessionId} durationMs={s.duration_ms} compact />
        </div>
      )}

      <div className="flex flex-col gap-1 rounded-xl bg-muted/60 px-4 py-3 text-[13px]">
        <span className="eyebrow">From the meeting</span>
        <span className="font-semibold">{task.title}</span>
        <span className="text-ink-soft">
          {task.folder || "Vault root"} · {formatRelative(task.startedAt)}
        </span>
      </div>

      <div className="mt-auto flex gap-2">
        <button
          type="button"
          className="btn-ghost flex-1"
          onClick={() => go({ kind: "session", id: task.sessionId, at: task.at ?? undefined })}
        >
          Open meeting <ArrowUpRight className="size-3.5" />
        </button>
        <button
          type="button"
          className="btn-primary flex-1"
          disabled={toggle.isPending}
          onClick={() => toggle.mutate(task)}
        >
          <CircleCheck className="size-4" /> {task.done ? "Reopen" : "Mark done"}
        </button>
      </div>
      {toggle.error && <p className="text-[12px] text-accent">{String(toggle.error)}</p>}
    </div>
  );
}

export function ActionsView() {
  const go = useNav((s) => s.go);
  const { tasks, userName, error } = useTasks();
  const today = useMemo(() => new Date(), []);
  const mine = tasks.filter((t) => t.mine && !t.done);
  const open = tasks.filter((t) => !t.done);
  const done = tasks.filter((t) => t.done);
  const overdue = open.filter((t) => groupOf(t, today) === "overdue");
  const [chosen, setChosen] = useState<Filter | null>(null);
  // Yours first when there are any.
  const filter: Filter = chosen ?? (mine.length > 0 ? "mine" : "open");
  const [owner, setOwner] = useState<string>("");
  const [selectedKey, setSelectedKey] = useState<string | null>(null);

  const owners = useMemo(() => {
    const counts = new Map<string, number>();
    for (const t of open) for (const o of t.owners) counts.set(o, (counts.get(o) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([name]) => name);
  }, [open]);

  const shown = (filter === "mine" ? mine : filter === "open" ? open : done).filter(
    (t) => !owner || t.owners.includes(owner),
  );
  // In the order they're listed: by group, soonest due first within one.
  const groups =
    filter === "done"
      ? []
      : GROUPS.map((g) => {
          const items = shown.filter((t) => groupOf(t, today) === g.id);
          if (g.id !== "undated") items.sort((a, b) => (a.due ?? "").localeCompare(b.due ?? ""));
          return { ...g, items };
        }).filter((g) => g.items.length > 0);
  const listed = filter === "done" ? shown : groups.flatMap((g) => g.items);
  const selected = tasks.find((t) => t.key === selectedKey) ?? listed[0] ?? null;

  const tabs: { id: Filter; label: string; count: number }[] = [
    { id: "mine", label: "Assigned to me", count: mine.length },
    { id: "open", label: "All open", count: open.length },
    { id: "done", label: "Done", count: done.length },
  ];

  const row = (t: Task) => (
    <TaskRow
      key={t.key}
      task={t}
      today={today}
      selected={selected?.key === t.key}
      onSelect={() => setSelectedKey(t.key)}
    />
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar crumbs={[...folderCrumbs(null), "Action items"]} />
      <div className="grid min-h-0 flex-1 grid-cols-[1fr_380px] gap-6 px-8 pb-6">
        <div className="min-h-0 overflow-y-auto pr-2">
          <div className="mx-auto flex max-w-[760px] flex-col gap-6 pt-2 pb-10">
            <PageHeader
              eyebrow="From your meeting notes"
              title="Action items"
              subtitle="Every task agreed in your meetings. Ticking one here ticks it in that meeting's notes too."
            />
            {error && <p className="text-accent">{String(error)}</p>}

            <div className="grid grid-cols-4 gap-3">
              <Stat label="Mine, open" value={mine.length} />
              <Stat label="Overdue" value={overdue.length} tone="alert" />
              <Stat label="All open" value={open.length} />
              <Stat label="Done" value={done.length} />
            </div>

            {!userName && (
              <p className="rounded-xl bg-sidebar px-4 py-3 text-[13px] text-ink-soft">
                Tasks written for “Me” count as yours. To include the ones given to you by name,{" "}
                <button
                  type="button"
                  className="font-semibold text-accent underline"
                  onClick={() => go({ kind: "settings" })}
                >
                  add your name in Settings
                </button>
                .
              </p>
            )}

            <div className="flex flex-wrap items-center gap-2">
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => {
                    setChosen(tab.id);
                    setSelectedKey(null);
                  }}
                  className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[13px] font-medium ${
                    filter === tab.id ? "bg-ink text-canvas" : "bg-muted text-ink hover:bg-high"
                  }`}
                >
                  {tab.label}
                  <span className="tabular-nums opacity-70">{tab.count}</span>
                </button>
              ))}
              {owners.length > 0 && filter !== "mine" && (
                <select
                  className="input ml-auto py-1.5 text-[13px]"
                  aria-label="Owner"
                  value={owner}
                  onChange={(e) => setOwner(e.target.value)}
                >
                  <option value="">Everyone</option>
                  {owners.map((o) => (
                    <option key={o} value={o}>
                      {o}
                    </option>
                  ))}
                </select>
              )}
            </div>

            {tasks.length === 0 && (
              <div className="card flex flex-col items-start gap-2 p-6">
                <p className="font-serif text-xl">No action items yet</p>
                <p className="text-ink-soft">
                  When Murmur writes a meeting's notes, the tasks people agreed on show up here.
                </p>
              </div>
            )}
            {tasks.length > 0 && shown.length === 0 && (
              <p className="text-ink-soft">
                {filter === "done" ? "Nothing ticked off yet." : "Nothing open here. Nice."}
              </p>
            )}

            {filter === "done"
              ? shown.length > 0 && <div className="flex flex-col gap-2">{shown.map(row)}</div>
              : groups.map(({ items, ...g }) => {
                  return (
                    <section key={g.id} className="flex flex-col gap-2">
                      <div className="flex items-baseline justify-between">
                        <h2 className="flex items-center gap-2 font-serif text-xl">
                          {g.id === "overdue" && <span className="size-2 rounded-full bg-accent" />}
                          {g.title}
                          <span className="chip bg-muted text-ink-soft">{items.length}</span>
                        </h2>
                        <span className="text-[12px] text-ink-soft">{g.hint}</span>
                      </div>
                      {items.map(row)}
                    </section>
                  );
                })}
          </div>
        </div>
        <div className="flex min-h-0 flex-col">
          {selected ? (
            <TaskDetail key={selected.key} task={selected} today={today} />
          ) : (
            <div className="card p-5 text-[13px] text-ink-soft">
              Select a task to see what was said.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
