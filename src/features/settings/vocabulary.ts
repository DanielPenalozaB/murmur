/** Splits the vocabulary field into terms: commas or lines, trimmed, no duplicates. */
export function parseVocabulary(text: string): string[] {
  const seen = new Set<string>();
  const terms: string[] = [];
  for (const raw of text.split(/[,\n]/)) {
    const term = raw.trim();
    const key = term.toLowerCase();
    if (term && !seen.has(key)) {
      seen.add(key);
      terms.push(term);
    }
  }
  return terms;
}
