const escapeHtml = (value: string) => value.replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]!);

export function highlightSearch(text: string, query: string): string {
  if (!query) return escapeHtml(text);
  const lower = text.toLocaleLowerCase();
  const needle = query.toLocaleLowerCase();
  const starts: number[] = [];
  const ends: number[] = [];
  if (lower.length !== text.length) {
    let offset = 0;
    for (const char of text) {
      for (let i = 0; i < char.toLocaleLowerCase().length; i++) {
        starts.push(offset);
        ends.push(offset + char.length);
      }
      offset += char.length;
    }
  }
  let result = "";
  let start = 0;
  let from = 0;
  for (let index = lower.indexOf(needle, from); index !== -1; index = lower.indexOf(needle, from)) {
    const matchStart = Math.max(start, starts[index] ?? index);
    const matchEnd = ends[index + needle.length - 1] ?? index + needle.length;
    if (matchEnd > start) {
      result += escapeHtml(text.slice(start, matchStart));
      result += `<mark class="search-hit">${escapeHtml(text.slice(matchStart, matchEnd))}</mark>`;
      start = matchEnd;
    }
    from = index + needle.length;
  }
  return result + escapeHtml(text.slice(start));
}
