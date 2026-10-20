import type { FileItem, Kind, PanelSession, RecordItem } from "./types";
export const labels: Record<Kind, string> = { text: "文本", link: "链接", image: "图片", file: "文件", video: "视频", color: "颜色" };
/** Only CR and LF at the end are removed; spaces, indentation and internal lines remain exact. */
export function textForUse(body: string, removeTrailingNewlines = true): string {
  return removeTrailingNewlines ? body.replace(/[\r\n]+$/, "") : body;
}
export function hasTrailingNewline(body: string): boolean { return /[\r\n]$/.test(body); }
export function filteredRecords(records: RecordItem[], { view, kindFilter, search }: PanelSession): RecordItem[] {
  return records.filter(record => {
    if (view === "favorite" && !record.favorite) return false;
    if (view.startsWith("group:") && record.groupId !== Number(view.slice(6))) return false;
    if (view === "default" && kindFilter !== "all" && record.kind !== kindFilter) return false;
    const text = `${record.title ?? ""} ${record.body} ${record.files.map(file => file.original).join(" ")}`.toLocaleLowerCase();
    return text.includes(search.toLocaleLowerCase());
  });
}
export function firstLineText(body: string): string {
  const lines = body.split(/\r\n|\r|\n/);
  const first = lines[0];
  const hasMore = lines.slice(1).some(line => line.trim() !== "");
  if (!first) return hasMore ? "..." : "空文本";
  return hasMore ? `${first} ...` : first;
}

export function recordListText(record: RecordItem, search: string, removeTrailingNewlines = true): string {
  const body = textForUse(record.body, removeTrailingNewlines);
  const usual = record.files.length > 1 ? `${record.files.length} 个文件 · ${record.files.map(file => file.original.split(/[\\/]/).pop()).join("、")}`
    : record.files.length === 1 ? record.files[0].original.split(/[\\/]/).pop() || "文件"
    : firstLineText(body);
  if (!search) return usual;
  const needle = search.toLocaleLowerCase();
  if (![body, ...record.files.map(file => file.original)].some(text => text.toLocaleLowerCase().includes(needle))) return usual;
  const source = [body, ...record.files.map(file => file.original)].find(text => text.toLocaleLowerCase().includes(needle))
    ?? `${body} ${record.files.map(file => file.original).join(" ")}`;
  return source;
}

export function fileTagText(files: FileItem[]): string {
  const extensions = files.map(file => {
    if (file.isDirectory) return "目录";
    const name = file.original.split(/[\\/]/).pop() || "";
    const dot = name.lastIndexOf(".");
    return dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toUpperCase() : "文件";
  });
  return extensions.every(extension => extension === extensions[0]) ? extensions[0] || "文件" : "多格式";
}

export function previewPaths(record: RecordItem): FileItem[] {
  if (record.files.length || !record.assetPath) return record.files;
  return [{ original: record.assetPath, stored: null, isDirectory: false }];
}
export function selectRange(records: readonly RecordItem[], anchorId: string | null, id: string, extend: boolean) {
  const anchor = extend ? records.findIndex(record => record.id === anchorId) : -1;
  const target = records.findIndex(record => record.id === id);
  if (target < 0) return null;
  const selectedIds = anchor < 0 ? new Set([id]) : new Set(records.slice(Math.min(anchor, target), Math.max(anchor, target) + 1).map(record => record.id));
  return { selectedId: id, selectedIds, anchorId: anchor < 0 ? id : anchorId };
}
