import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { linkCitations } from "./citations";

/** Renders model output. Timestamp citations become chips that call `onSeek`. */
export function Markdown({ text, onSeek }: { text: string; onSeek?: (ms: number) => void }) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => {
            const ms = href?.startsWith("#t=") ? Number(href.slice(3)) : null;
            if (ms === null) return <span className="underline">{children}</span>;
            return (
              <button
                type="button"
                className="mx-0.5 rounded bg-accent-soft px-1.5 py-px text-[12px] font-medium tabular-nums text-on-accent-soft hover:ring-1 hover:ring-accent/50"
                onClick={() => onSeek?.(ms)}
                title="Show in transcript"
              >
                {children}
              </button>
            );
          },
        }}
      >
        {linkCitations(text)}
      </ReactMarkdown>
    </div>
  );
}
