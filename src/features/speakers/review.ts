import type { QueryClient } from "@tanstack/react-query";
import type { ReviewItem, ReviewSpeaker } from "../../lib/ipc";

/** Speakers with one name usually sound at least this alike (see SAME_NAME_BELOW). */
export const SAME_PERSON = 0.5;

export function percent(score: number) {
  return `${Math.round(Math.max(0, score) * 100)}%`;
}

function meeting(s: ReviewSpeaker) {
  return `“${s.title}”`;
}

/** A headline and a plain-words explanation of a review finding. */
export function describe(item: ReviewItem): { title: string; detail: string } {
  const usual = `The same person usually sounds over ${percent(SAME_PERSON)} alike.`;
  switch (item.kind) {
    case "pair": {
      const [a, b] = item.speakers;
      const where =
        a.session_id === b.session_id
          ? `In ${meeting(a)}, ${a.label} and ${b.label} sound ${percent(item.score)} alike.`
          : `${a.label} in ${meeting(a)} and ${b.label} in ${meeting(b)} sound ${percent(item.score)} alike.`;
      return {
        title: `Two different voices named ${item.name}?`,
        detail: `${where} ${usual} One of them is probably someone else.`,
      };
    }
    case "outlier":
      return {
        title: `${item.speaker.label} in ${meeting(item.speaker)} may not be ${item.name}`,
        detail: `They sound ${percent(item.score)} like ${item.name} in other meetings. ${usual}`,
      };
    case "look_alike": {
      const [a, b] = item.names;
      return {
        title: `${a} and ${b} sound ${percent(item.score)} alike`,
        detail: `If they're the same person, rename one to the other's name. If not, Murmur may mix them up in later meetings.`,
      };
    }
  }
}

/** After a name changes, everything that shows speakers is out of date. */
export function refreshSpeakers(queryClient: QueryClient, sessionId?: string) {
  const keys = [["sessions"], ["voices"], ["voice-sessions"], ["speaker-review"], ["tasks"]];
  if (sessionId) keys.push(["session", sessionId]);
  return Promise.all(keys.map((queryKey) => queryClient.invalidateQueries({ queryKey })));
}
