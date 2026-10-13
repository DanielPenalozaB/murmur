const TIME = /(?:\d{1,2}:)?\d{1,2}:\d{2}/g;
/** `[12:04]`, `[1:02:03]`, `[00:31, 00:39]` or `[12:04–12:30]`. */
const CITATION = /\[((?:\d{1,2}:)?\d{1,2}:\d{2}(?:\s*[,–-]\s*(?:\d{1,2}:)?\d{1,2}:\d{2})*)\]/g;

export function parseTimestamp(t: string) {
  return t.split(":").reduce((total, part) => total * 60 + Number(part), 0) * 1000;
}

/** Turns timestamp citations into `#t=<ms>` links so they can jump to the transcript. */
export function linkCitations(markdown: string) {
  return markdown.replace(CITATION, (_, inner: string) =>
    inner.replace(TIME, (t) => `[${t}](#t=${parseTimestamp(t)})`),
  );
}
