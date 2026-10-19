type Measure = (text: string) => number;

// Prepare the match once; resizing only measures short candidates near the hit.
export function prepareSearchExcerpt(text: string, query: string): (width: number, measure: Measure) => string {
  const lower = text.toLocaleLowerCase();
  const needle = query.toLocaleLowerCase();
  const index = lower.indexOf(needle);
  if (!query || index < 0) return () => text;

  let first = index, last = index + needle.length;
  if (lower.length !== text.length) {
    let folded = 0, offset = 0;
    for (const char of text) {
      const next = folded + char.toLocaleLowerCase().length;
      if (folded <= index && index < next) first = offset;
      offset += char.length;
      if (next >= index + needle.length) { last = offset; break; }
      folded = next;
    }
  }
  const insidePair = (offset: number) => offset > 0 && offset < text.length
    && text.charCodeAt(offset - 1) >= 0xd800 && text.charCodeAt(offset - 1) <= 0xdbff
    && text.charCodeAt(offset) >= 0xdc00 && text.charCodeAt(offset) <= 0xdfff;
  const before = (offset: number) => insidePair(offset) ? offset - 1 : offset;
  const after = (offset: number) => insidePair(offset) ? offset + 1 : offset;
  first = before(first);
  last = after(last);
  const excerpt = (start: number, end: number) => `${start ? "…" : ""}${text.slice(start, end)}${end < text.length ? "…" : ""}`;

  // Grow from a small candidate instead of starting binary search at half of a
  // potentially multi-megabyte body. Work scales with what the row can display.
  const largest = (limit: number, fits: (size: number) => boolean) => {
    let low = 0, high = Math.min(1, limit);
    while (high < limit && fits(high)) {
      low = high;
      high = Math.min(limit, high * 2);
    }
    while (low < high) {
      const mid = Math.ceil((low + high) / 2);
      if (fits(mid)) low = mid;
      else high = mid - 1;
    }
    return low;
  };
  return (width, measure) => {
    if (width <= 0) return "";
    // Check the beginning without an ellipsis so short content that just fits
    // remains complete, without ever measuring the whole of a long body.
    if (largest(text.length, size => measure(text.slice(0, after(size))) <= width) === text.length) return text;
    const preceding = largest(first, size => measure(excerpt(before(first - size), last)) <= width);
    let start = preceding === first ? 0
      : before(first - largest(first, size => measure(text.slice(before(first - size), first)) <= width / 3));
    if (measure(excerpt(start, last)) > width) start = before(first - preceding);
    let end = after(last + largest(text.length - last, size => measure(excerpt(start, after(last + size))) <= width));
    start = before(start - largest(start, size => measure(excerpt(before(start - size), end)) <= width));
    end = after(end + largest(text.length - end, size => measure(excerpt(start, after(end + size))) <= width));
    return excerpt(start, end);
  };
}

// Width comes from the rendered row, so short content is never shortened in advance.
export function searchExcerpt(text: string, query: string, width: number, measure: Measure): string {
  return prepareSearchExcerpt(text, query)(width, measure);
}
