export function listOffsets(records: readonly { kind: string }[]): number[] {
  const offsets = [0];
  for (const record of records) offsets.push(offsets[offsets.length - 1] + (record.kind === "image" || record.kind === "video" ? 180 : 48));
  return offsets;
}

function firstRowAfter(offsets: readonly number[], position: number): number {
  let low = 0;
  let high = offsets.length - 1;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (offsets[middle + 1] <= position) low = middle + 1;
    else high = middle;
  }
  return low;
}

export function visibleRange(offsets: readonly number[], scrollTop: number, height: number, overscan = 180): { start: number; end: number } {
  const count = offsets.length - 1;
  if (!count) return { start: 0, end: 0 };
  const start = firstRowAfter(offsets, Math.max(0, scrollTop - overscan));
  const bottom = Math.max(0, scrollTop + height + overscan);
  let end = firstRowAfter(offsets, bottom);
  if (end < count && offsets[end] < bottom) end++;
  return { start, end: Math.max(start, end) };
}

export function rowScrollTop(offsets: readonly number[], index: number, scrollTop: number, height: number): number {
  if (offsets[index] < scrollTop) return offsets[index];
  if (offsets[index + 1] > scrollTop + height) return Math.max(0, offsets[index + 1] - height);
  return scrollTop;
}

export function backToTopOffset(offsets: readonly number[], height: number): number | null {
  let capacity = 0;
  while (capacity < offsets.length - 1 && offsets[capacity + 1] <= height) capacity++;
  return capacity + 9 < offsets.length - 1 ? offsets[capacity + 9] : null;
}
