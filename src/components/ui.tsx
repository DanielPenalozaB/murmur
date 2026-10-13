import { AudioLines } from "lucide-react";
import type { ReactNode } from "react";

/** "Elena Rostova" → "ER", "Speaker 2" → "S2", "Me" → "ME". */
export function initials(name: string) {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}

const AVATAR_TONES = [
  "bg-accent-soft text-on-accent-soft",
  "bg-local-soft text-on-local-soft",
  "bg-warm-soft text-warm",
  "bg-high text-ink",
];

/** Same name, same color, everywhere. */
export function avatarTone(name: string) {
  if (name === "Me") return "bg-accent text-on-accent";
  let hash = 0;
  for (const c of name) hash = (hash * 31 + c.charCodeAt(0)) >>> 0;
  return AVATAR_TONES[hash % AVATAR_TONES.length];
}

export function Avatar({ name, size = "sm" }: { name: string; size?: "xs" | "sm" | "lg" }) {
  const dims = { xs: "size-5 text-[9px]", sm: "size-7 text-[11px]", lg: "size-10 text-[13px]" }[
    size
  ];
  return (
    <span
      className={`inline-flex shrink-0 items-center justify-center rounded-full font-bold ring-2 ring-canvas ${dims} ${avatarTone(name)}`}
      title={name}
    >
      {initials(name)}
    </span>
  );
}

/** Overlapping avatars for a list of people. */
export function AvatarStack({ names, max = 4 }: { names: string[]; max?: number }) {
  return (
    <span className="flex -space-x-1">
      {names.slice(0, max).map((n) => (
        <Avatar key={n} name={n} size="xs" />
      ))}
    </span>
  );
}

export function LogoMark({ size = "md" }: { size?: "sm" | "md" }) {
  const dims = size === "sm" ? "size-7 rounded-md" : "size-8 rounded-lg";
  return (
    <span className={`inline-flex items-center justify-center bg-ink text-canvas ${dims}`}>
      <AudioLines className="size-4" strokeWidth={2.25} />
    </span>
  );
}

/** Window-top bar: logo, breadcrumb, and optional actions. Doubles as the drag area. */
export function TopBar({ crumbs, children }: { crumbs: string[]; children?: ReactNode }) {
  return (
    <header
      className="flex h-14 shrink-0 items-center gap-3 px-6 text-[13px]"
      data-tauri-drag-region
    >
      <LogoMark size="sm" />
      <nav className="flex min-w-0 items-center gap-2 text-ink-soft" data-tauri-drag-region>
        {crumbs.map((c, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: a crumb's position is its identity
          <span key={`${i}-${c}`} className="flex min-w-0 items-center gap-2">
            {i > 0 && <span className="text-outline">/</span>}
            <span className={`truncate ${i === crumbs.length - 1 ? "font-medium text-ink" : ""}`}>
              {c}
            </span>
          </span>
        ))}
      </nav>
      <div className="ml-auto flex items-center gap-2">{children}</div>
    </header>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 shrink-0 rounded-full transition-colors ${checked ? "bg-accent" : "bg-high"}`}
    >
      <span
        className={`absolute top-0.5 size-5 rounded-full bg-surface shadow transition-all ${checked ? "left-[22px]" : "left-0.5"}`}
      />
    </button>
  );
}

/** Page heading block used by library-style views. */
export function PageHeader({
  eyebrow,
  title,
  subtitle,
  children,
}: {
  eyebrow: ReactNode;
  title: string;
  subtitle?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        <p className="eyebrow flex items-center gap-2">{eyebrow}</p>
        <h1 className="mt-1 font-serif text-[40px] leading-[48px] tracking-[-0.02em]">{title}</h1>
        {subtitle && <p className="mt-1 text-ink-soft">{subtitle}</p>}
      </div>
      {children}
    </div>
  );
}
