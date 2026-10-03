import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { cursorPosition, currentMonitor, getCurrentWindow, PhysicalSize, monitorFromPoint, PhysicalPosition } from "@tauri-apps/api/window";
import { highlightSearch } from "./search-highlight";
import { ensurePreviewWindow, initPreviewWindows } from "./preview-windows";
import { backToTopOffset, listOffsets, rowScrollTop, visibleRange } from "./list-layout";
import "./styles.css";

type Kind = "text" | "link" | "image" | "file" | "video" | "color";
type FileItem = { original: string; stored: string | null; isDirectory: boolean };
type RecordItem = {
  id: string; kind: Kind; body: string; files: FileItem[]; favorite: boolean;
  pinnedAt: number | null;
  groupId: number | null; usedAt: number; createdAt: number | null; expiresAt: number | null;
  status: string; preview: boolean; assetPath: string | null;
};
type Group = { id: number; name: string };
type Settings = {
  maxOrdinary: number; retentionDays: number; sensitiveSeconds: number; shortcut: string;
  positionMode: "fixed" | "cursor"; fixedPosition: "left" | "right" | "center";
  rememberPosition: boolean; panelWidth: number; panelHeight: number;
  panelX: number | null; panelY: number | null;
  alwaysOnTop: boolean;
};
type Snapshot = { records: RecordItem[]; groups: Group[]; settings: Settings };
type PanelSession = { view: string; kindFilter: string; search: string };
type FocusTarget = { kind: "id" | "record" | "view"; value: string };

const panel = getCurrentWindow();
const isMacOS = navigator.userAgent.includes("Macintosh");
const root = document.querySelector<HTMLDivElement>("#app")!;
let state: Snapshot;
let panelActive = false;
let panelGeneration = 0;
let openingPanel: Promise<void> | null = null;
let refreshing: Promise<void> | null = null;
let refreshAgain = false;
let view = "default";
let kindFilter = "all";
let search = "";
let selectedId: string | null = null;
let selectedIds = new Set<string>();
let selectionAnchorId: string | null = null;
let moveGroupIds: string[] = [];
let pendingShortcutId: string | null = null;
let settingsOpen = false;
let settingsReturnFocus: FocusTarget | null = null;
let clearingCache = false;
let cacheSizeText = "计算中…";
let cacheSizeRequest = 0;
let groupEdit: { mode: "create" | "rename"; id: number | null } | null = null;
let groupSaving = false;
let contextGroupId: number | null = null;
let activeShortcut = "";
let shortcutError: string | null = null;
let shortcutDraft: string | null = null;
let autostartEnabled: boolean | null = null;
let autostartSaving = false;
let positionSaveTimer: number | undefined;
const numberSaveTimers = new Map<string, number>();
let settingsSave: Promise<void> = Promise.resolve();
let placing = false;
let lastResizePress: { edge: string; time: number; x: number; y: number } | null = null;
let hoveredId: string | null = null;
let previewFromKeyboard = false;
let previewId: string | null = null;
let previewHovered = false;
let largePreviewId: string | null = null;
let largePreviewSizeRequest = 0;
let hoverTimer: number | undefined;
let previewCloseTimer: number | undefined;
let previewGeneration = 0;
let previewPlacement: { x: number; rowY: number; width: number; scale: number; top: number; bottom: number } | null = null;
let thumbnailObserver: IntersectionObserver;
let listRecords: RecordItem[] = [];
let offsets = [0];
let renderedRange = { start: -1, end: -1 };
let backToTopPosition: number | null = null;
let backToTopFrame = 0;

const labels: Record<Kind, string> = { text: "文本", link: "链接", image: "图片", file: "文件", video: "视频", color: "颜色" };
const displayShortcut = (shortcut: string) => shortcut.split("+").map(part => part === "Control" ? "Ctrl" : part).join("+");
const escapeHtml = (value: string) => value.replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]!);
const call = async <T>(name: string, args: Record<string, unknown> = {}): Promise<T> => invoke<T>(name, args);
const icon = (name: string, size = 14) => `<svg width="${size}" height="${size}" aria-hidden="true"><use href="#lu-${name}"/></svg>`;

function toast(message: string) {
  const el = document.querySelector<HTMLElement>("#toast")!;
  el.textContent = message;
  el.classList.add("visible");
  window.setTimeout(() => el.classList.remove("visible"), 3500);
}

function savePanelSession() {
  void call("save_panel_session", { session: { view, kindFilter, search } }).catch(() => {});
}

async function action(name: string, args: Record<string, unknown> = {}) {
  try { await call(name, args); if (name === "hide_panel") releasePanelContent(); await refresh(); }
  catch (error) { toast(String(error)); }
}

function filtered(): RecordItem[] {
  return state.records.filter(record => {
    if (view === "favorite" && !record.favorite) return false;
    if (view.startsWith("group:") && record.groupId !== Number(view.slice(6))) return false;
    if (view === "default" && kindFilter !== "all" && record.kind !== kindFilter) return false;
    const text = `${record.body} ${record.files.map(file => file.original).join(" ")}`.toLocaleLowerCase();
    return text.includes(search.toLocaleLowerCase());
  });
}

function renderGroups() {
  if (!panelActive) return;
  const tabs = document.querySelector<HTMLElement>("#groups")!;
  if (!groupEdit) {
    const groups = [{ id: "default", name: "默认" }, { id: "favorite", name: "收藏" },
      ...state.groups.map(group => ({ id: `group:${group.id}`, name: group.name }))];
    tabs.innerHTML = groups.map(group => `<button class="group ${view === group.id ? "active" : ""}" data-view="${group.id}" data-label="${escapeHtml(group.name)}">${escapeHtml(group.name)}</button>`).join("")
      + `<button class="add-group" title="添加分组" aria-label="添加分组" id="add-group">${icon("plus", 14)}</button>`;
  }
  if (view !== "default") setFilterMenuOpen(false);
  renderFilter();
}

function selectGroup(nextView: string) {
  view = nextView;
  savePanelSession();
  selectedId = null;
  selectedIds.clear();
  selectionAnchorId = null;
  document.querySelector<HTMLElement>("#list")!.scrollTop = 0;
  document.querySelectorAll<HTMLElement>("#groups [data-view]").forEach(group => group.classList.toggle("active", group.dataset.view === view));
  if (view !== "default") setFilterMenuOpen(false);
  renderFilter();
  renderList();
}

function closeGroupMenu() {
  document.querySelector<HTMLElement>("#group-menu")!.hidden = true;
  contextGroupId = null;
}

function closeRecordMenu() {
  document.querySelector<HTMLElement>("#record-menu")!.hidden = true;
}

function startGroupEdit(mode: "create" | "rename", id: number | null = null) {
  if (groupEdit) return;
  const target = mode === "create" ? document.querySelector<HTMLElement>("#add-group")
    : document.querySelector<HTMLElement>(`#groups [data-view="group:${id}"]`);
  if (!target) return;
  const input = document.createElement("input");
  input.className = "group-input";
  input.maxLength = 40;
  input.setAttribute("aria-label", mode === "create" ? "新分组名称" : "重命名分组");
  input.placeholder = mode === "create" ? "分组名称" : "";
  input.value = mode === "rename" ? state.groups.find(group => group.id === id)?.name ?? "" : "";
  groupEdit = { mode, id };
  target.replaceWith(input);
  input.focus();
  if (mode === "rename") input.select();
}

function cancelGroupEdit() {
  if (!groupEdit || groupSaving) return;
  groupEdit = null;
  renderGroups();
  updateBackToTopButton();
}

async function commitGroupEdit() {
  if (!groupEdit || groupSaving) return;
  const edit = groupEdit;
  const input = document.querySelector<HTMLInputElement>("#groups .group-input")!;
  const name = input.value.trim();
  if (edit.mode === "rename" && name === state.groups.find(group => group.id === edit.id)?.name) { cancelGroupEdit(); return; }
  if (!name || name === "默认" || name === "收藏" || state.groups.some(group => group.name === name && group.id !== edit.id)) {
    toast("分组名称不可用");
    input.focus();
    input.select();
    return;
  }
  groupSaving = true;
  try {
    await call(edit.mode === "create" ? "add_group" : "rename_group", edit.mode === "create" ? { name } : { id: edit.id, name });
    groupEdit = null;
    await refresh();
  } catch (error) {
    toast(String(error));
    input.focus();
    input.select();
  } finally { groupSaving = false; }
}

function setFilterMenuOpen(open: boolean) {
  document.querySelector<HTMLElement>("#filter-menu")!.hidden = !open;
  document.querySelector<HTMLElement>("#kind-filter")!.setAttribute("aria-expanded", String(open));
}

function setRetentionMenuOpen(open: boolean) {
  const menu = document.querySelector<HTMLElement>("#retention-menu");
  const trigger = document.querySelector<HTMLElement>("#retention-trigger");
  if (!menu || !trigger) return;
  menu.hidden = !open;
  trigger.setAttribute("aria-expanded", String(open));
}

function setFixedPositionMenuOpen(open: boolean) {
  const menu = document.querySelector<HTMLElement>("#fixed-position-menu");
  const trigger = document.querySelector<HTMLElement>("#fixed-position-trigger");
  if (!menu || !trigger) return;
  menu.hidden = !open;
  trigger.setAttribute("aria-expanded", String(open));
}

function closeMoveGroupMenu() {
  document.querySelector<HTMLElement>("#move-group-menu")!.hidden = true;
  document.querySelector<HTMLElement>("[data-move-group-trigger][aria-expanded='true']")?.setAttribute("aria-expanded", "false");
  moveGroupIds = [];
}

function openMoveGroupMenu(trigger: HTMLElement, anchor = trigger, records = [state.records.find(record => record.id === trigger.dataset.moveGroupTrigger)!]) {
  const menu = document.querySelector<HTMLElement>("#move-group-menu")!;
  menu.dataset.recordId = trigger.dataset.moveGroupTrigger!;
  moveGroupIds = records.map(record => record.id);
  menu.innerHTML = [{ id: null, name: "未分组" }, ...state.groups].map(group =>
    `<button type="button" role="menuitemradio" data-move-group="${group.id ?? ""}" aria-checked="${records.every(record => record.groupId === group.id)}">${escapeHtml(group.name)}</button>`).join("");
  menu.hidden = false;
  trigger.setAttribute("aria-expanded", "true");
  const shell = document.querySelector<HTMLElement>(".shell")!;
  const bounds = shell.getBoundingClientRect();
  const button = anchor.getBoundingClientRect();
  menu.style.left = `${Math.max(8, Math.min(button.right - bounds.left - menu.offsetWidth, shell.clientWidth - menu.offsetWidth - 8))}px`;
  const below = button.bottom - bounds.top + 4;
  const above = button.top - bounds.top - menu.offsetHeight - 4;
  menu.style.top = `${below + menu.offsetHeight <= shell.clientHeight - 8 ? below : Math.max(8, above)}px`;
  (menu.querySelector<HTMLButtonElement>("[aria-checked='true']") ?? menu.querySelector<HTMLButtonElement>("button"))?.focus();
}

function renderFilter() {
  const button = document.querySelector<HTMLButtonElement>("#kind-filter")!;
  const disabled = view !== "default";
  const selected = !disabled && kindFilter !== "all";
  button.disabled = disabled;
  button.innerHTML = icon(selected ? kindFilter : "filter", 14);
  button.title = disabled ? "仅默认分组可筛选" : selected ? `类型筛选：${labels[kindFilter as Kind]}` : "类型筛选：全部";
  button.setAttribute("aria-label", button.title);
  button.classList.toggle("active", selected);
  document.querySelector<HTMLElement>("#clear-filter")!.hidden = !selected;
  document.querySelector<HTMLElement>("#filter-wrap")!.classList.toggle("active", selected);
  document.querySelector<HTMLElement>("#filter-wrap")!.classList.toggle("disabled", disabled);
  document.querySelectorAll<HTMLElement>("[data-kind]").forEach(option => option.classList.toggle("active", option.dataset.kind === kindFilter));
}

// 列表只显示正文首行。首行之后还有内容时在末尾以空格加省略标记，提示这条记录没有完整显示；其余行只有空白时不加标记。
// 首行本身为空（正文以换行开头）时省略标记前没有可依附的空格，直接显示省略标记。
function firstLineText(body: string): string {
  const lines = body.split(/\r\n|\r|\n/);
  const first = lines[0];
  const hasMore = lines.slice(1).some(line => line.trim() !== "");
  if (!first) return hasMore ? "..." : "空文本";
  return hasMore ? `${first} ...` : first;
}

function recordListText(record: RecordItem): string {
  const usual = record.files.length > 1 ? `${record.files.length} 个文件 · ${record.files.map(file => file.original.split(/[\\/]/).pop()).join("、")}`
    : record.files.length === 1 ? record.files[0].original.split(/[\\/]/).pop() || "文件"
    : firstLineText(record.body);
  if (!search) return usual;
  const needle = search.toLocaleLowerCase();
  const source = [record.body, ...record.files.map(file => file.original)].find(text => text.toLocaleLowerCase().includes(needle))
    ?? `${record.body} ${record.files.map(file => file.original).join(" ")}`;
  const index = source.toLocaleLowerCase().indexOf(needle);
  const start = Math.max(0, index - 6);
  const end = Math.min(source.length, index + search.length + 48);
  return `${start ? "…" : ""}${source.slice(start, end)}${end < source.length ? "…" : ""}`;
}

function fileTagText(files: FileItem[]): string {
  const extensions = files.map(file => {
    if (file.isDirectory) return "目录";
    const name = file.original.split(/[\\/]/).pop() || "";
    const dot = name.lastIndexOf(".");
    return dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toUpperCase() : "文件";
  });
  return extensions.every(extension => extension === extensions[0]) ? extensions[0] || "文件" : "多格式";
}

function closePreview() {
  if (previewId) void emitTo("preview", "preview-clear", previewGeneration).catch(() => {});
  window.clearTimeout(hoverTimer);
  window.clearTimeout(previewCloseTimer);
  hoveredId = null;
  previewFromKeyboard = false;
  previewId = null;
  previewHovered = false;
  previewPlacement = null;
  previewGeneration++;
  void call("hide_preview").catch(() => {});
}

function schedulePreviewClose() {
  window.clearTimeout(previewCloseTimer);
  previewCloseTimer = window.setTimeout(() => {
    if (!hoveredId && !previewHovered) closePreview();
  }, 220);
}

// 清空搜索词并恢复当前视图的完整列表；Esc 与工具栏的清空按钮共用这一行为。
// 清空按钮把焦点留在搜索框便于继续输入；Esc 清空后移走搜索框焦点，与“未聚焦时输入即搜索”的规则一致。
function clearSearch(refocus = true) {
  const input = document.querySelector<HTMLInputElement>("#search")!;
  input.value = "";
  search = "";
  savePanelSession();
  document.querySelector<HTMLButtonElement>("#clear-search")!.hidden = true;
  renderList();
  if (refocus) input.focus({ preventScroll: true });
  else input.blur();
}

function recordRow(id: string): HTMLElement | null {
  return [...document.querySelectorAll<HTMLElement>("#list .record")].find(item => item.dataset.id === id) ?? null;
}

function ensureRecordRow(id: string): HTMLElement | null {
  const index = listRecords.findIndex(record => record.id === id);
  if (index < 0) return null;
  const list = document.querySelector<HTMLElement>("#list")!;
  list.scrollTop = rowScrollTop(offsets, index, list.scrollTop, list.clientHeight - 8);
  renderListRange();
  return recordRow(id);
}

// 悬停预览的路径列表：文件、图片和视频记录使用原路径；
// 直接复制的图片（截图等只有位图的复制）没有原文件，改用本工具保存的图片副本路径。
function previewPaths(record: RecordItem): FileItem[] {
  if (record.files.length || !record.assetPath) return record.files;
  return [{ original: record.assetPath, stored: null, isDirectory: false }];
}

async function preparePreview(id: string) {
  if (hoveredId !== id) return;
  const token = ++previewGeneration;
  const row = recordRow(id);
  const record = state.records.find(item => item.id === id);
  if (!row || !record) return;
  try {
    await ensurePreviewWindow("preview");
    if (token !== previewGeneration || hoveredId !== id) {
      if (!hoveredId) void call("hide_preview").catch(() => {});
      return;
    }
    const [position, size, scale] = await Promise.all([panel.outerPosition(), panel.outerSize(), panel.scaleFactor()]);
    const monitor = await monitorFromPoint(position.x + size.width / 2, position.y + size.height / 2);
    if (!monitor || token !== previewGeneration || hoveredId !== id) return;
    const area = monitor.workArea;
    const margin = Math.round(12 * scale);
    const gap = Math.round(2 * scale);
    const right = area.position.x + area.size.width - margin;
    const left = area.position.x + margin;
    const desired = Math.round(420 * scale);
    const minimum = Math.round(260 * scale);
    const rightSpace = right - position.x - size.width - gap;
    const leftSpace = position.x - gap - left;
    const useRight = rightSpace >= desired || (leftSpace < desired && rightSpace >= leftSpace);
    const available = useRight ? rightSpace : leftSpace;
    const width = Math.max(1, Math.min(desired, available >= minimum ? available : right - left));
    const anchorX = useRight ? position.x + size.width + gap : position.x - gap - width;
    const x = Math.max(left, Math.min(anchorX, right - width));
    const rowY = position.y + Math.round(row.getBoundingClientRect().top * scale);
    previewId = id;
    previewPlacement = { x, rowY, width, scale, top: area.position.y, bottom: area.position.y + area.size.height };
    await emitTo("preview", "preview-content", {
      id, kind: record.kind, body: record.body, files: previewPaths(record),
      groupName: state.groups.find(group => group.id === record.groupId)?.name ?? null,
      preview: record.preview, status: record.status, search, width: width / scale,
      maxHeight: (area.size.height - 2 * margin) / scale,
      generation: token,
    });
  } catch (error) {
    if (token !== previewGeneration) return;
    void call("hide_preview").catch(() => {});
    toast(`预览失败：${String(error)}`);
  }
}

function hoverRecord(id: string) {
  if (largePreviewId) return;
  if (hoveredId === id) return;
  closePreview();
  hoveredId = id;
  hoverTimer = window.setTimeout(() => { void preparePreview(id); }, 300);
}

// 键盘触发悬浮预览：与鼠标悬停共用定位与显示流程，但不等待 300 毫秒。
// 选中记录滚出可视区时先滚入列表，再等一帧启动，避免 #list 的滚动监听把刚打开的预览关掉。
function showHoverPreview(id: string) {
  if (largePreviewId || !state || hoveredId === id) return;
  const row = ensureRecordRow(id);
  if (!row) return;
  row.scrollIntoView({ block: "nearest" });
  window.requestAnimationFrame(() => {
    if (largePreviewId || hoveredId === id) return;
    if (!recordRow(id) || !state.records.some(item => item.id === id)) return;
    closePreview();
    hoveredId = id;
    previewFromKeyboard = true;
    window.clearTimeout(hoverTimer);
    hoverTimer = window.setTimeout(() => { void preparePreview(id); }, 0);
  });
}

function markSelectedRows() {
  const rows = document.querySelectorAll<HTMLElement>("#list .record");
  rows.forEach(row => {
    const selected = selectedIds.has(row.dataset.id!);
    row.classList.toggle("selected", selected);
    row.classList.toggle("before-selected", selectedIds.has(listRecords[Number(row.dataset.index) + 1]?.id ?? ""));
  });
}

function selectRecord(id: string, extend = false) {
  const records = filtered();
  const anchor = extend ? records.findIndex(record => record.id === selectionAnchorId) : -1;
  const target = records.findIndex(record => record.id === id);
  if (target < 0) return;
  selectedIds = anchor < 0 ? new Set([id])
    : new Set(records.slice(Math.min(anchor, target), Math.max(anchor, target) + 1).map(record => record.id));
  selectedId = id;
  if (anchor < 0) selectionAnchorId = id;
  markSelectedRows();
}

function renderList() {
  if (!panelActive) return;
  window.cancelAnimationFrame(backToTopFrame);
  backToTopFrame = 0;
  closePreview();
  closeMoveGroupMenu();
  closeRecordMenu();
  const list = document.querySelector<HTMLElement>("#list")!;
  const records = filtered();
  const visibleIds = new Set(records.map(record => record.id));
  selectedIds = new Set([...selectedIds].filter(id => visibleIds.has(id)));
  if (!selectedId || !visibleIds.has(selectedId)) selectedId = records.find(record => selectedIds.has(record.id))?.id ?? records[0]?.id ?? null;
  if (selectedId && !selectedIds.has(selectedId)) selectedIds = new Set([selectedId]);
  if (!selectionAnchorId || !visibleIds.has(selectionAnchorId)) selectionAnchorId = selectedId;
  if (largePreviewId && !records.some(record => record.id === largePreviewId)) {
    largePreviewId = null;
    void call("hide_large_preview").catch(() => {});
  }
  listRecords = records;
  offsets = listOffsets(records);
  renderedRange = { start: -1, end: -1 };
  const maximumScroll = Math.max(0, offsets[offsets.length - 1] + 8 - list.clientHeight);
  list.scrollTop = Math.min(list.scrollTop, maximumScroll);
  renderListRange();
  updateBackToTopTarget();
}

function renderListRange() {
  if (!panelActive) return;
  const list = document.querySelector<HTMLElement>("#list")!;
  const scrollTop = list.scrollTop;
  const { start, end } = visibleRange(offsets, Math.max(0, scrollTop - 2), list.clientHeight - 8);
  if (renderedRange.start === start && renderedRange.end === end) { updateOrdinals(); return; }
  renderedRange = { start, end };
  thumbnailObserver?.disconnect();
  const previousImages = new Map([...list.querySelectorAll<HTMLImageElement>(".preview[data-preview]")]
    .map(image => [image.dataset.preview!, image]));
  const records = listRecords;
  const nextList = document.createElement("template");
  nextList.innerHTML = records.length ? `<div class="list-spacer" style="height:${offsets[start]}px" aria-hidden="true"></div>` + records.slice(start, end).map((record, index) => {
    const status = record.status === "pending" ? "保存副本中" : record.status === "missing" ? "文件已不可用" : record.status === "copy_failed" ? "副本保存失败" : "";
    const swatch = record.kind === "color" && /^#(?:[\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8})$/i.test(record.body.trim());
    const media = record.kind === "image" || record.kind === "video";
    const hasPreview = record.preview || (record.kind === "image" && record.files.length > 0 && record.status !== "pending");
    const text = media && !search ? "" : recordListText(record);
    const fileTag = record.kind === "file" ? escapeHtml(fileTagText(record.files)) : "";
    const kindIcon = media ? "" : swatch ? `<span class="swatch" style="background-color:${record.body.trim()}" title="颜色"></span>`
      : record.kind === "text" ? "" : record.kind === "file" ? `<span class="file-tag" title="${fileTag}">${fileTag}</span>`
      : `<span class="kind-icon ${record.kind}">${icon(record.kind, 12)}</span>`;
    const note = status || (record.expiresAt ? "敏感记录" : "");
    return `<article class="record ${selectedIds.has(record.id) ? "selected" : ""} ${media ? "media-record" : ""} ${start + index === records.length - 1 ? "last-record" : ""}" data-id="${record.id}" data-index="${start + index}" tabindex="-1">
      <span class="ordinal"></span>
      ${media ? `<span class="preview-wrap">${hasPreview ? `<img class="preview" data-preview="${record.id}" alt="${labels[record.kind]}预览" />${record.kind === "video" ? `<span class="play">▶</span>` : ""}` : `<span class="preview-unavailable">${record.kind === "video" ? "首帧预览不可用" : "图片预览不可用"}</span>`}</span>${search ? `<span class="media-search-match" style="bottom:${note ? 32 : 9}px">${highlightSearch(text, search)}</span>` : ""}${note ? `<span class="media-note">${note}</span>` : ""}`
        : `<div class="record-main"><div class="record-content">${kindIcon}<span class="record-text">${highlightSearch(text, search)}</span>${note ? `<span class="record-note">${note}</span>` : ""}</div></div>`}
      <span class="record-indicators">${record.pinnedAt !== null ? `<span class="record-indicator pin-indicator" aria-label="已置顶" title="已置顶">${icon("pin", 13)}</span>` : ""}
      ${record.favorite ? `<span class="record-indicator favorite-indicator" aria-label="已收藏" title="已收藏">${icon("star-fill", 13)}</span>` : ""}
      ${record.groupId !== null ? `<span class="record-indicator group-indicator" aria-label="已分组" title="已分组">${icon("group", 13)}</span>` : ""}</span>
      <div class="record-actions" role="group" aria-label="记录操作"><button class="record-pin ${record.pinnedAt !== null ? "on" : ""}" data-record-pin="${record.id}" title="${record.pinnedAt !== null ? "取消置顶记录" : "置顶记录"}" aria-label="${record.pinnedAt !== null ? "取消置顶记录" : "置顶记录"}" aria-pressed="${record.pinnedAt !== null}">${icon("pin", 13)}</button>
      <button class="favorite ${record.favorite ? "on" : ""}" data-favorite="${record.id}" title="${record.favorite ? "取消收藏" : "收藏"}" aria-label="${record.favorite ? "取消收藏" : "收藏"}">${icon(record.favorite ? "star-fill" : "star", 13)}</button>
      <button class="move-group ${record.groupId !== null ? "on" : ""}" type="button" data-move-group-trigger="${record.id}" title="移动到分组" aria-label="移动到分组" aria-haspopup="menu" aria-expanded="false">${icon("group", 13)}</button>
      <button class="delete" data-delete="${record.id}" title="删除记录" aria-label="删除记录">${icon("x", 12)}</button></div></article>`;
  }).join("") + `<div class="list-spacer" style="height:${offsets[records.length] - offsets[end]}px" aria-hidden="true"></div>` : `<div class="empty">这里还没有记录</div>`;
  nextList.content.querySelectorAll<HTMLImageElement>(".preview[data-preview]").forEach(image => {
    const previous = previousImages.get(image.dataset.preview!);
    if (previous) { image.replaceWith(previous); previousImages.delete(image.dataset.preview!); }
  });
  previousImages.forEach(image => image.removeAttribute("src"));
  list.replaceChildren(nextList.content);
  markSelectedRows();
  list.scrollTop = scrollTop;
  updateOrdinals();
  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const image = entry.target as HTMLImageElement;
      observer.unobserve(image);
      image.dataset.loading = "true";
      void call<string>("preview_data", { id: image.dataset.preview, role: "thumbnail" }).then(src => {
        if (!panelActive || !image.isConnected || !list.contains(image)) return;
        image.src = src; delete image.dataset.loading;
      }).catch(() => {
        if (!panelActive || !image.isConnected || !list.contains(image)) return;
        image.closest(".preview-wrap")?.replaceChildren(Object.assign(document.createElement("span"), { className: "preview-unavailable", textContent: "预览图不可用" }));
      });
    }
  }, { root: list, rootMargin: "180px 0px" });
  thumbnailObserver = observer;
  list.querySelectorAll<HTMLImageElement>("[data-preview]").forEach(image => {
    if (!image.hasAttribute("src") && !image.dataset.loading) observer.observe(image);
  });
}

function updateBackToTopTarget() {
  const list = document.querySelector<HTMLElement>("#list")!;
  const style = getComputedStyle(list);
  const availableHeight = list.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
  backToTopPosition = backToTopOffset(offsets, availableHeight);
  updateBackToTopButton();
}

async function openLargePreview(id: string) {
  if (largePreviewId) return;
  const record = state.records.find(item => item.id === id);
  if (!record || (record.kind !== "image" && record.kind !== "video")) return;
  if (record.kind === "video" && !record.preview && record.files.length < 2) { toast("首帧预览不可用"); return; }
  closePreview();
  largePreviewId = id;
  const request = ++largePreviewSizeRequest;
  try {
    await ensurePreviewWindow("large-preview");
    if (largePreviewId !== id || request !== largePreviewSizeRequest) {
      if (!largePreviewId) void call("hide_large_preview").catch(() => {});
      return;
    }
    await emitTo("large-preview", "large-preview-content", { id, kind: record.kind, count: Math.max(1, record.files.length) });
  }
  catch (error) {
    if (largePreviewId !== id || request !== largePreviewSizeRequest) return;
    largePreviewId = null;
    void call("hide_large_preview").catch(() => {});
    toast(`预览失败：${String(error)}`);
  }
}

function visibleRecords(): HTMLElement[] {
  const list = document.querySelector<HTMLElement>("#list")!;
  const bounds = list.getBoundingClientRect();
  return [...list.querySelectorAll<HTMLElement>(".record")].filter(row => {
    const rect = row.getBoundingClientRect();
    return rect.bottom > bounds.top + 1 && rect.top < bounds.bottom - 1;
  });
}

function updateOrdinals() {
  const list = document.querySelector<HTMLElement>("#list")!;
  list.querySelectorAll<HTMLElement>(".ordinal").forEach(el => el.textContent = "");
  visibleRecords().slice(0, 9).forEach((row, index) => row.querySelector<HTMLElement>(".ordinal")!.textContent = String(index + 1));
}

function updateBackToTopButton() {
  const active = document.querySelector<HTMLButtonElement>("#groups .group.active");
  const previous = document.querySelector<HTMLButtonElement>("#groups .group.back-to-top");
  if (previous && previous !== active) {
    previous.classList.remove("back-to-top");
    previous.textContent = previous.dataset.label!;
    previous.style.removeProperty("width");
    previous.removeAttribute("title");
    previous.removeAttribute("aria-label");
  }
  if (!active) return;
  const list = document.querySelector<HTMLElement>("#list")!;
  const show = backToTopPosition !== null && backToTopPosition + 2 - list.scrollTop < list.clientHeight;
  if (active.classList.contains("back-to-top") === show) return;
  if (show) active.style.width = `${active.getBoundingClientRect().width}px`;
  active.classList.toggle("back-to-top", show);
  active.innerHTML = show ? icon("arrow-up-to-line", 15) : escapeHtml(active.dataset.label!);
  if (show) {
    active.title = "返回顶部";
    active.setAttribute("aria-label", "返回顶部");
  } else {
    active.style.removeProperty("width");
    active.removeAttribute("title");
    active.removeAttribute("aria-label");
  }
}

function scrollBackToTop() {
  const list = document.querySelector<HTMLElement>("#list")!;
  window.cancelAnimationFrame(backToTopFrame);
  backToTopFrame = 0;
  const start = list.scrollTop;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) { list.scrollTop = 0; return; }
  const duration = Math.min(700, 320 + Math.sqrt(start) * 12);
  const startedAt = performance.now();
  const step = (now: number) => {
    const progress = Math.min(1, (now - startedAt) / duration);
    const eased = progress < .5 ? 2 * progress * progress : 1 - 2 * (1 - progress) * (1 - progress);
    list.scrollTop = start * (1 - eased);
    if (progress < 1) backToTopFrame = window.requestAnimationFrame(step);
    else backToTopFrame = 0;
  };
  backToTopFrame = window.requestAnimationFrame(step);
}

function formatCacheSize(bytes: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = bytes;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit++; }
  return `${unit === 0 ? size : size.toFixed(1)} ${units[unit]}`;
}

async function refreshCacheSize() {
  const request = ++cacheSizeRequest;
  try {
    const bytes = await call<number>("get_cache_size");
    if (request !== cacheSizeRequest || !settingsOpen) return;
    cacheSizeText = formatCacheSize(bytes);
  } catch {
    if (request !== cacheSizeRequest || !settingsOpen) return;
    cacheSizeText = "无法获取";
  }
  const size = document.querySelector<HTMLElement>("#cache-size");
  if (size) size.textContent = cacheSizeText;
}

function renderSettings() {
  if (!panelActive) return;
  const settings = document.querySelector<HTMLElement>("#settings")!;
  settings.hidden = !settingsOpen;
  document.querySelector<HTMLElement>("#settings-button")!.classList.toggle("active", settingsOpen);
  if (!settingsOpen) return;
  const s = state.settings;
  settings.innerHTML = `<h2>设置</h2><label class="inline"><input id="autostart" type="checkbox" ${autostartEnabled ? "checked" : ""} ${autostartEnabled === null || autostartSaving ? "disabled" : ""}>开机启动</label>
    <label>唤出快捷键<input id="shortcut" placeholder="点击后按组合键" value="${escapeHtml(displayShortcut(shortcutDraft ?? s.shortcut))}" readonly>${shortcutError ? `<span class="shortcut-error" role="alert">${escapeHtml(shortcutError)}</span>` : ""}</label>
    <label>普通记录数量上限<input id="max-ordinary" type="number" min="1" max="500" value="${s.maxOrdinary}"></label>
    <div class="setting-field"><span class="setting-label">普通记录保留时间</span><div class="setting-select retention-select">
      <button id="retention-trigger" type="button" aria-haspopup="listbox" aria-expanded="false"><span class="retention-value">${([[1,"24 小时"],[3,"3 天"],[5,"5 天"],[7,"7 天"],[30,"1 个月"]].find(([value]) => value === s.retentionDays) ?? [s.retentionDays, `${s.retentionDays} 天`])[1]}</span>${icon("chevron", 14)}</button>
      <div id="retention-menu" role="listbox" hidden>${[[1,"24 小时"],[3,"3 天"],[5,"5 天"],[7,"7 天"],[30,"1 个月"]].map(([value,label]) => `<button type="button" role="option" data-retention="${value}" aria-selected="${s.retentionDays === value}">${label}</button>`).join("")}</div>
    </div></div>
    <label>1Password 记录保留秒数<input id="sensitive" type="number" min="1" max="4294967295" value="${s.sensitiveSeconds}"></label>
    <p class="hint">数量和时间上限只作用于普通记录。收藏或已分组的记录只能手动删除。</p>
    <fieldset><legend>面板位置</legend><label class="inline"><input type="radio" name="mode" value="fixed" ${s.positionMode === "fixed" ? "checked" : ""}>固定位置</label>
    <label class="inline"><input type="radio" name="mode" value="cursor" ${s.positionMode === "cursor" ? "checked" : ""}>跟随光标</label></fieldset>
    <div class="setting-field"><span class="setting-label">固定位置</span><div class="setting-select fixed-position-select">
      <button id="fixed-position-trigger" type="button" aria-haspopup="listbox" aria-expanded="false" ${s.positionMode === "cursor" ? "disabled" : ""}><span class="fixed-position-value">${{ left: "左下角", right: "右下角", center: "下方居中" }[s.fixedPosition]}</span>${icon("chevron", 14)}</button>
      <div id="fixed-position-menu" role="listbox" hidden>${[["left","左下角"],["right","右下角"],["center","下方居中"]].map(([value,label]) => `<button type="button" role="option" data-fixed-position="${value}" aria-selected="${s.fixedPosition === value}">${label}</button>`).join("")}</div>
    </div></div>
    <label class="inline"><input id="remember" type="checkbox" ${s.rememberPosition ? "checked" : ""} ${s.positionMode === "cursor" ? "disabled" : ""}>记忆拖拽位置</label>
    <p class="hint">拖拽面板边缘调整大小；缩放光标出现时，双击左右边缘恢复默认宽度，双击上下边缘恢复默认高度，双击角落恢复两维。大小会在下次打开时恢复。</p>
    <h3>快捷键</h3><table class="shortcut-table"><thead><tr><th scope="col">按键</th><th scope="col">作用</th></tr></thead><tbody>
      <tr><td>${s.shortcut ? escapeHtml(displayShortcut(s.shortcut)) : "未设置"}</td><td>唤出面板（全局快捷键）</td></tr>
      <tr><td>直接输入</td><td>在主页搜索当前分组</td></tr>
      <tr><td>Shift+Esc</td><td>聚焦搜索框</td></tr>
      <tr><td>Backspace / Delete</td><td>搜索框聚焦时编辑搜索词；未聚焦时 Backspace 也可编辑搜索词</td></tr>
      <tr><td>Tab / Shift+Tab</td><td>切换到下一个 / 上一个分组</td></tr>
      <tr><td>←</td><td>选中图片或视频记录时打开大图预览</td></tr>
      <tr><td>→</td><td>显示当前选中记录的悬浮预览</td></tr>
      <tr><td>↑ / ↓</td><td>选择上一条 / 下一条记录，首尾循环；分组菜单中切换选项</td></tr>
      <tr><td>Shift+↑ / Shift+↓、Shift+点击</td><td>按当前列表顺序连续选择多条记录</td></tr>
      <tr><td>Enter</td><td>应用选中记录；分组菜单中确认选择；编辑分组名时提交</td></tr>
      <tr><td>Space</td><td>打开或关闭大图预览；分组菜单中确认选择</td></tr>
      <tr><td>Ctrl+1～9</td><td>应用当前可见序号对应的记录</td></tr>
      <tr><td>Ctrl+D / Delete</td><td>删除所有选中记录（搜索框聚焦时仅 Ctrl+D）</td></tr>
      <tr><td>Ctrl+F</td><td>收藏或取消收藏所有选中记录</td></tr>
      <tr><td>Ctrl+G</td><td>打开分组菜单，移动所有选中记录</td></tr>
      <tr><td>Ctrl+P</td><td>置顶或取消置顶所有选中记录</td></tr>
      <tr><td>Ctrl+A</td><td>面板中禁用全选；搜索框聚焦时全选搜索文字</td></tr>
      <tr><td>Esc</td><td>关闭当前菜单、设置或大图预览；搜索词非空时清除搜索词并移走搜索框焦点，面板保持显示；搜索框为空时在主页隐藏面板；编辑分组名时取消</td></tr>
    </tbody></table>
    <section class="cache-cleanup" aria-labelledby="cache-cleanup-title"><div class="cache-cleanup-copy"><h3 id="cache-cleanup-title">清理缓存</h3>
      <p class="hint">删除所有普通记录、对应的本地文件及孤立附件。收藏和自建分组中的记录会保留。当前可清理缓存大小：<span id="cache-size">${cacheSizeText}</span>。此操作无法撤销。</p></div>
      <button id="clear-cache" class="primary" ${clearingCache ? "disabled" : ""}>${clearingCache ? "清理中…" : "清理缓存"}</button></section>`;
}

// 记下当前被选中的对象，用于离开设置后原样还回去。列表会重绘，所以只记“身份”而不是元素本身。
function focusTargetOf(element: Element | null): FocusTarget | null {
  const el = element as HTMLElement | null;
  if (!el || el === document.body || el === document.documentElement) return null;
  if (el.closest("#settings-button")) return null;
  const record = el.closest<HTMLElement>(".record[data-id]");
  if (record?.dataset.id) return { kind: "record", value: record.dataset.id };
  const view = el.closest<HTMLElement>("#groups [data-view]");
  if (view?.dataset.view) return { kind: "view", value: view.dataset.view };
  if (el.id) return { kind: "id", value: el.id };
  return null;
}

function resolveFocusTarget(target: FocusTarget | null): HTMLElement | null {
  if (!target) return null;
  if (target.kind === "id") return document.getElementById(target.value);
  if (target.kind === "record") return ensureRecordRow(target.value);
  return document.querySelector<HTMLElement>(`#groups [data-view="${CSS.escape(target.value)}"]`);
}

// 关闭设置：先还原进入设置前的选中对象；对象已不存在时退回搜索框；本来就无所指则移走选中，避免按钮留框。
function closeSettings() {
  settingsOpen = false;
  renderSettings();
  const previous = settingsReturnFocus;
  settingsReturnFocus = null;
  const target = resolveFocusTarget(previous) ?? (previous ? document.querySelector<HTMLInputElement>("#search") : null);
  if (target) {
    target.focus({ preventScroll: true });
    if (target.classList.contains("record")) target.scrollIntoView({ block: "nearest" });
  } else if (document.activeElement instanceof HTMLElement) {
    document.activeElement.blur();
  }
}

function render() {
  if (!panelActive) return;
  renderGroups(); renderList(); renderSettings();
  const pin = document.querySelector<HTMLButtonElement>("#pin")!;
  pin.setAttribute("aria-pressed", String(state.settings.alwaysOnTop));
  pin.title = state.settings.alwaysOnTop ? "取消置顶" : "置顶";
  pin.setAttribute("aria-label", pin.title);
}
function refresh(): Promise<void> {
  if (!panelActive) return Promise.resolve();
  refreshAgain = true;
  if (refreshing) return refreshing;
  refreshing = (async () => {
    while (refreshAgain && panelActive) {
      refreshAgain = false;
      const generation = panelGeneration;
      const next = await call<Snapshot>("get_state");
      if (!panelActive || generation !== panelGeneration) continue;
      state = next;
      if (view.startsWith("group:") && !state.groups.some(group => `group:${group.id}` === view)) { view = "default"; savePanelSession(); }
      render();
      if (settingsOpen) void refreshCacheSize();
    }
  })().finally(() => { refreshing = null; });
  return refreshing;
}

function releasePanelContent() {
  if (!panelActive) return;
  panelActive = false;
  panelGeneration++;
  refreshAgain = false;
  savePanelSession();
  window.cancelAnimationFrame(backToTopFrame);
  backToTopFrame = 0;
  closePreview();
  largePreviewId = null;
  largePreviewSizeRequest++;
  settingsOpen = false;
  settingsReturnFocus = null;
  groupEdit = null;
  pendingShortcutId = null;
  selectedId = null;
  selectedIds.clear();
  selectionAnchorId = null;
  closeGroupMenu();
  closeMoveGroupMenu();
  closeRecordMenu();
  setFilterMenuOpen(false);
  thumbnailObserver?.disconnect();
  const list = document.querySelector<HTMLElement>("#list")!;
  list.querySelectorAll<HTMLImageElement>("img").forEach(image => image.removeAttribute("src"));
  list.replaceChildren();
  document.querySelector<HTMLElement>("#groups")!.replaceChildren();
  const settings = document.querySelector<HTMLElement>("#settings")!;
  settings.hidden = true;
  settings.replaceChildren();
  listRecords = [];
  offsets = [0];
  renderedRange = { start: -1, end: -1 };
  backToTopPosition = null;
  if (state) state.records = [];
}

async function apply(id: string) {
  try {
    toast(await call<string>("apply_record", { id }));
    if (!await panel.isVisible()) releasePanelContent();
    await refresh();
  }
  catch (error) { toast(String(error)); }
}

function cycleGroup(direction: number) {
  const views = ["default", "favorite", ...state.groups.map(group => `group:${group.id}`)];
  selectGroup(views[(views.indexOf(view) + direction + views.length) % views.length]);
  const active = document.querySelector<HTMLElement>("#groups .group.active");
  active?.scrollIntoView({ block: "nearest", inline: "nearest" });
  active?.focus({ preventScroll: true });
}

function moveSelection(direction: number, extend = false) {
  window.cancelAnimationFrame(backToTopFrame);
  backToTopFrame = 0;
  const records = filtered();
  if (!records.length) return;
  const index = records.findIndex(record => record.id === selectedId);
  const nextIndex = index < 0 ? 0 : extend
    ? Math.max(0, Math.min(records.length - 1, index + direction))
    : (index + direction + records.length) % records.length;
  const id = records[nextIndex].id;
  selectRecord(id, extend);
  // 键盘触发的悬浮预览跟随选中记录：选中变化时关闭，鼠标悬停触发的预览不受影响。
  if (previewFromKeyboard) closePreview();
  const row = ensureRecordRow(id);
  row?.scrollIntoView({ block: "nearest" });
  row?.focus({ preventScroll: true });
  updateOrdinals();
}

async function placePanel() {
  const s = state.settings;
  placing = true;
  await panel.setSize(new PhysicalSize(s.panelWidth, s.panelHeight));
  const size = await panel.outerSize();
  const cursor = await cursorPosition();
  const monitor = s.positionMode === "cursor" ? await monitorFromPoint(cursor.x, cursor.y) : await currentMonitor();
  if (monitor) {
    const area = monitor.workArea;
    let x = area.position.x + area.size.width - size.width - 18;
    let y = area.position.y + area.size.height - size.height - 18;
    if (s.positionMode === "cursor") {
      x = Math.min(cursor.x + 12, x); y = Math.min(cursor.y + 12, y);
    } else if (s.rememberPosition && s.panelX !== null && s.panelY !== null) {
      x = s.panelX; y = s.panelY;
    } else if (s.fixedPosition === "left") {
      x = area.position.x + 18;
    } else if (s.fixedPosition === "center") {
      x = area.position.x + Math.round((area.size.width - size.width) / 2);
    }
    await panel.setPosition(new PhysicalPosition(x, y));
  }
  window.setTimeout(() => placing = false, 350);
}

async function registerShortcut(shortcut: string) {
  if (shortcut === activeShortcut) return;
  await call("register_panel_shortcut", { shortcut });
  activeShortcut = shortcut;
}

function shortcutFailure(shortcut: string, error: unknown) {
  const detail = String(error);
  if (!shortcut) return `无法清除快捷键：${detail}`;
  return /already registered/i.test(detail)
    ? `${displayShortcut(shortcut)} 已被占用，请更换组合键或解除占用。`
    : `无法启用 ${displayShortcut(shortcut)}：${detail}`;
}

function saveSetting(patch: Partial<Settings>, reposition = false) {
  settingsSave = settingsSave.then(async () => {
    const previous = state.settings;
    const next = { ...previous, ...patch };
    if (Object.keys(patch).every(key => previous[key as keyof Settings] === next[key as keyof Settings])
      && (patch.shortcut === undefined || patch.shortcut === activeShortcut)) {
      if (patch.shortcut !== undefined && shortcutDraft === next.shortcut) shortcutDraft = null;
      return;
    }
    try {
      if (patch.rememberPosition && !previous.rememberPosition) {
        const position = await panel.outerPosition();
        next.panelX = position.x; next.panelY = position.y;
      }
      if (patch.shortcut !== undefined) await registerShortcut(next.shortcut);
      await call("save_settings", { value: next });
      state.settings = next;
      if (patch.shortcut !== undefined) {
        if (shortcutDraft === next.shortcut) shortcutDraft = null;
        shortcutError = null;
        renderSettings();
      }
    } catch (error) {
      if (patch.shortcut !== undefined) await registerShortcut(previous.shortcut).catch(() => {});
      if (patch.shortcut !== undefined && shortcutDraft === next.shortcut) shortcutError = shortcutFailure(next.shortcut, error);
      renderSettings();
      toast(shortcutError ?? String(error));
      return;
    }
    if (reposition) await placePanel().catch(error => toast(String(error)));
  });
}

function saveNumberSetting(element: HTMLInputElement, restoreInvalid = false) {
  const key = element.id === "max-ordinary" ? "maxOrdinary" : "sensitiveSeconds";
  const value = Number(element.value);
  if (!element.value || !element.validity.valid || !Number.isInteger(value)) {
    if (restoreInvalid) { element.value = String(state.settings[key]); toast("请输入有效的设置值"); }
    return;
  }
  saveSetting({ [key]: value });
}

function openPanel(): Promise<void> {
  if (openingPanel) return panelActive ? openingPanel : openingPanel.then(() => openPanel());
  panelActive = true;
  const generation = ++panelGeneration;
  openingPanel = (async () => {
    if (settingsOpen) closeSettings();
    selectedId = null;
    selectedIds.clear();
    selectionAnchorId = null;
    document.querySelector<HTMLElement>("#list")!.scrollTop = 0;
    document.querySelector<HTMLInputElement>("#search")!.value = search;
    document.querySelector<HTMLButtonElement>("#clear-search")!.hidden = !search;
    await refresh();
    if (!panelActive || generation !== panelGeneration) return;
    activeShortcut = shortcutError ? "" : state.settings.shortcut;
    await placePanel();
    if (!panelActive || generation !== panelGeneration) return;
    await call("show_panel");
  })().finally(() => { openingPanel = null; });
  return openingPanel;
}

function shortcutFromKey(event: KeyboardEvent): string | null {
  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
  if (["Control", "Alt", "Shift", "Meta"].includes(key)) return null;
  const modifiers = [event.ctrlKey && "Control", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Super"].filter(Boolean);
  if (!modifiers.length || !/^([A-Z0-9]|F\d{1,2})$/.test(key)) return null;
  return [...modifiers, key].join("+");
}

root.innerHTML = `<div class="shell"><div class="resize-edge left" data-resize-edge="left"></div><div class="resize-edge right" data-resize-edge="right"></div><div class="resize-edge top" data-resize-edge="top"></div><div class="resize-edge bottom" data-resize-edge="bottom"></div>
  <svg class="svg-defs" aria-hidden="true"><defs>
    <symbol id="lu-star" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.12 2.12 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.12 2.12 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.12 2.12 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.12 2.12 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.12 2.12 0 0 0 1.597-1.16z"/></symbol>
    <symbol id="lu-star-fill" viewBox="0 0 24 24"><path fill="currentColor" d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.12 2.12 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.12 2.12 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.12 2.12 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.12 2.12 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.12 2.12 0 0 0 1.597-1.16z"/></symbol>
    <symbol id="lu-pin" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M12 17v5M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4a1 1 0 0 1 1 1z"/></symbol>
    <symbol id="lu-gear" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8"><path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0a2.34 2.34 0 0 0 3.319 1.915a2.34 2.34 0 0 1 2.33 4.033a2.34 2.34 0 0 0 0 3.831a2.34 2.34 0 0 1-2.33 4.033a2.34 2.34 0 0 0-3.319 1.915a2.34 2.34 0 0 1-4.659 0a2.34 2.34 0 0 0-3.32-1.915a2.34 2.34 0 0 1-2.33-4.033a2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/><circle cx="12" cy="12" r="3"/></g></symbol>
    <symbol id="lu-plus" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" d="M5 12h14m-7-7v14"/></symbol>
    <symbol id="lu-arrow-up-to-line" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M12 19V7m-5 5 5-5 5 5M4 3h16"/></symbol>
    <symbol id="lu-x" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" d="M18 6L6 18M6 6l12 12"/></symbol>
    <symbol id="lu-minus" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" d="M5 12h14"/></symbol>
    <symbol id="lu-square" viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/></symbol>
    <symbol id="lu-search" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.8"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/></g></symbol>
    <symbol id="lu-chevron" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m6 9 6 6 6-6"/></symbol>
    <symbol id="lu-link" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></g></symbol>
    <symbol id="lu-image" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8"><rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21"/></g></symbol>
    <symbol id="lu-file" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></g></symbol>
    <symbol id="lu-video" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8"><path d="m22 8-6 4 6 4V8Z"/><rect x="2" y="6" width="14" height="12" rx="2"/></g></symbol>
    <symbol id="lu-group" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linejoin="round" stroke-width="1.8"><rect x="3" y="3" width="13" height="13" rx="2"/><rect x="8" y="8" width="13" height="13" rx="2"/></g></symbol>
    <symbol id="lu-filter" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M22 3H2l8 9.46V19l4 2v-8.54Z"/></symbol>
    <symbol id="lu-text" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" d="M4 6h16M12 6v13M8 19h8"/></symbol>
    <symbol id="lu-color" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M12 3a9 9 0 0 1 0 18Z" fill="currentColor"/></symbol>
  </defs></svg>
  <header class="titlebar"><svg class="logo" viewBox="0 0 14 14" aria-hidden="true"><rect x="1" y="1" width="12" height="12" rx="3.5" fill="none" stroke="#4f46e5" stroke-width="2"/><rect x="4.6" y="4.6" width="4.8" height="4.8" rx="1.4" fill="#4f46e5"/></svg><span class="appname">TP</span>
    <div class="winbtns"><button id="minimize" class="winbtn" title="最小化" aria-label="最小化">${icon("minus", 13)}</button><button id="maximize" class="winbtn" title="最大化或还原" aria-label="最大化或还原">${icon("square", 11)}</button><button id="close" class="winbtn close" title="隐藏面板" aria-label="隐藏面板">${icon("x", 13)}</button></div>
  </header>
  <div class="toolbar"><div class="search-box"><label class="search-field" for="search">${icon("search", 13)}<input id="search" placeholder="搜索 · Shift+Esc" aria-label="搜索当前分组" autocomplete="off"></label><button id="clear-search" type="button" title="清空搜索" aria-label="清空搜索" hidden>${icon("x", 13)}</button></div>
    <div id="filter-wrap" class="filter-wrap"><div class="filter-control"><button id="kind-filter" class="iconbtn" title="类型筛选：全部" aria-label="类型筛选：全部" aria-haspopup="menu" aria-expanded="false">${icon("filter", 14)}</button><button id="clear-filter" class="clear-filter" title="清除筛选" aria-label="清除筛选" hidden>${icon("x", 11)}</button></div>
      <div id="filter-menu" class="filter-menu" role="menu" hidden>${(["all", "text", "image", "video", "file", "link", "color"] as const).map(kind => `<button data-kind="${kind}" role="menuitem">${icon(kind === "all" ? "filter" : kind, 14)}<span>${kind === "all" ? "全部" : labels[kind]}</span></button>`).join("")}</div></div>
    <button id="pin" class="iconbtn" aria-label="置顶" aria-pressed="false" title="置顶">${icon("pin", 15)}</button><button id="settings-button" class="iconbtn" aria-label="设置" title="设置">${icon("gear", 15)}</button></div>
  <nav id="groups" aria-label="分组"></nav><div id="group-menu" role="menu" hidden><button type="button" data-group-action="rename" role="menuitem">重命名</button><button type="button" data-group-action="delete" role="menuitem">删除</button></div><main id="list"></main><div id="record-menu" role="menu" aria-label="记录操作" hidden><button type="button" data-record-action="copy-paths" role="menuitem">复制路径</button><button type="button" data-record-action="open-directory" role="menuitem">打开所在目录</button><button type="button" data-record-action="save-as" role="menuitem">另存为</button></div><div id="move-group-menu" role="menu" aria-label="移动到分组" hidden></div><section id="settings" hidden></section>
  <div id="toast" role="status"></div></div>`;

document.querySelector<HTMLElement>(".titlebar")!.addEventListener("pointerdown", event => {
  if (event.button === 0 && !(event.target as HTMLElement).closest("button")) void panel.startDragging().catch(error => toast(String(error)));
});

root.querySelectorAll<HTMLElement>("[data-resize-edge]").forEach(handle => {
  handle.addEventListener("pointerdown", async event => {
    if (event.button !== 0) return;
    event.preventDefault();
    const edge = handle.dataset.resizeEdge!;
    const now = performance.now();
    const previous = lastResizePress;
    lastResizePress = { edge, time: now, x: event.screenX, y: event.screenY };
    if (previous?.edge === edge && now - previous.time < 500 &&
        Math.abs(event.screenX - previous.x) < 6 && Math.abs(event.screenY - previous.y) < 6) {
      lastResizePress = null;
      try {
        const [size, scale] = await Promise.all([panel.innerSize(), panel.scaleFactor()]);
        await panel.setSize(new PhysicalSize(edge === "left" || edge === "right" ? Math.round(420 * scale) : size.width,
          edge === "top" || edge === "bottom" ? Math.round(600 * scale) : size.height));
        scheduleGeometrySave();
      } catch (error) { toast(String(error)); }
      return;
    }
    if (isMacOS) {
      const start = { x: event.screenX, y: event.screenY };
      let latest = start;
      let initial: { size: PhysicalSize; position: PhysicalPosition; scale: number } | null = null;
      let applying = false;
      let ended = false;
      const resize = async () => {
        if (!initial || applying || (latest.x === start.x && latest.y === start.y)) return;
        applying = true;
        try {
          let at: typeof latest;
          do {
            at = latest;
            const dx = Math.round((at.x - start.x) * initial.scale);
            const dy = Math.round((at.y - start.y) * initial.scale);
            const width = initial.size.width + (edge === "right" ? dx : edge === "left" ? -dx : 0);
            const height = initial.size.height + (edge === "bottom" ? dy : edge === "top" ? -dy : 0);
            const nextWidth = Math.max(340 * initial.scale, width);
            const nextHeight = Math.max(400 * initial.scale, height);
            await panel.setSize(new PhysicalSize(nextWidth, nextHeight));
            if (edge === "left" || edge === "top") {
              await panel.setPosition(new PhysicalPosition(initial.position.x + (edge === "left" ? initial.size.width - nextWidth : 0),
                initial.position.y + (edge === "top" ? initial.size.height - nextHeight : 0)));
            }
          } while (at !== latest);
        } finally { applying = false; if (ended) scheduleGeometrySave(); }
      };
      const move = (next: PointerEvent) => {
        latest = { x: next.screenX, y: next.screenY };
        void resize().catch(error => toast(String(error)));
      };
      const end = (next: PointerEvent) => {
        move(next);
        ended = true;
        handle.removeEventListener("pointermove", move);
        handle.removeEventListener("pointerup", end);
        handle.removeEventListener("pointercancel", end);
        if (!applying && initial) scheduleGeometrySave();
      };
      handle.setPointerCapture(event.pointerId);
      handle.addEventListener("pointermove", move);
      handle.addEventListener("pointerup", end);
      handle.addEventListener("pointercancel", end);
      try {
        const [size, position, scale] = await Promise.all([panel.innerSize(), panel.outerPosition(), panel.scaleFactor()]);
        initial = { size, position, scale };
        await resize();
      } catch (error) { toast(String(error)); }
      return;
    }
    try {
      await panel.startResizeDragging(({ left: "West", right: "East", top: "North", bottom: "South" } as const)[edge as "left" | "right" | "top" | "bottom"]);
    } catch (error) { toast(String(error)); }
  });
});

document.querySelector<HTMLElement>("#groups")!.addEventListener("wheel", event => {
  const tabs = event.currentTarget as HTMLElement;
  if (event.shiftKey || !event.deltaY || tabs.scrollWidth <= tabs.clientWidth) return;
  event.preventDefault();
  const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? tabs.clientWidth : 1;
  tabs.scrollLeft += event.deltaY * unit;
}, { passive: false });

document.querySelector<HTMLElement>("#groups")!.addEventListener("contextmenu", event => {
  const target = event.target as HTMLElement;
  if (target.closest(".group-input")) return;
  event.preventDefault();
  closeGroupMenu();
  closeRecordMenu();
  const group = target.closest<HTMLElement>(".group[data-view^='group:']");
  if (!group) return;
  if (groupSaving) return;
  const id = Number(group.dataset.view!.slice(6));
  cancelGroupEdit();
  const menu = document.querySelector<HTMLElement>("#group-menu")!;
  const shell = document.querySelector<HTMLElement>(".shell")!;
  const bounds = shell.getBoundingClientRect();
  contextGroupId = id;
  menu.hidden = false;
  menu.style.left = `${Math.max(8, Math.min(event.clientX - bounds.left, shell.clientWidth - menu.offsetWidth - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(event.clientY - bounds.top, shell.clientHeight - menu.offsetHeight - 8))}px`;
});

document.querySelector<HTMLElement>("#list")!.addEventListener("contextmenu", event => {
  closeRecordMenu();
  const row = (event.target as HTMLElement).closest<HTMLElement>(".record");
  if (!row || !state) return;
  const record = state.records.find(item => item.id === row.dataset.id);
  if (!record) return;
  event.preventDefault();
  closeGroupMenu();
  closeMoveGroupMenu();
  if (["text", "link", "color"].includes(record.kind)) return;
  const hasFiles = record.files.length > 0 && record.files.every(file => !file.isDirectory);
  const directImage = record.kind === "image" && record.files.length === 0;
  if (!directImage && !hasFiles) return;
  closePreview();
  const menu = document.querySelector<HTMLElement>("#record-menu")!;
  const shell = document.querySelector<HTMLElement>(".shell")!;
  const bounds = shell.getBoundingClientRect();
  menu.dataset.recordId = record.id;
  menu.hidden = false;
  menu.style.left = `${Math.max(8, Math.min(event.clientX - bounds.left, shell.clientWidth - menu.offsetWidth - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(event.clientY - bounds.top, shell.clientHeight - menu.offsetHeight - 8))}px`;
  menu.querySelector<HTMLButtonElement>("button")!.focus();
});

root.addEventListener("focusout", event => {
  const input = (event.target as HTMLElement).closest<HTMLInputElement>("#groups .group-input");
  if (!input) return;
  window.setTimeout(() => { if (document.activeElement !== input) cancelGroupEdit(); }, 0);
});

// 鼠标按下设置按钮时，浏览器还没把选中位置挪到按钮上，此刻记下原来的选中对象。
root.addEventListener("pointerdown", event => {
  if (!settingsOpen && (event.target as HTMLElement).closest("#settings-button")) settingsReturnFocus = focusTargetOf(document.activeElement);
});

root.addEventListener("click", async event => {
  const element = event.target as HTMLElement;
  const recordAction = element.closest<HTMLElement>("[data-record-action]");
  if (recordAction) {
    const id = document.querySelector<HTMLElement>("#record-menu")!.dataset.recordId!;
    closeRecordMenu();
    try {
      if (recordAction.dataset.recordAction === "copy-paths") await call("copy_record_paths", { id });
      else if (recordAction.dataset.recordAction === "open-directory") await call("open_record_directory", { id });
      else if (recordAction.dataset.recordAction === "save-as") await call("save_record_files", { id });
    }
    catch (error) { toast(String(error)); }
    return;
  }
  if (!element.closest("#record-menu")) closeRecordMenu();
  const groupAction = element.closest<HTMLElement>("[data-group-action]");
  if (groupAction && contextGroupId !== null) {
    const id = contextGroupId;
    closeGroupMenu();
    if (groupAction.dataset.groupAction === "rename") startGroupEdit("rename", id);
    else {
      try {
        await call("delete_group", { id });
        if (view === `group:${id}`) view = "default";
        await refresh();
      } catch (error) { toast(String(error)); }
    }
    return;
  }
  if (!element.closest("#group-menu")) closeGroupMenu();
  const filterMenu = document.querySelector<HTMLElement>("#filter-menu")!;
  if (!element.closest("#filter-wrap")) setFilterMenuOpen(false);
  const retentionMenu = document.querySelector<HTMLElement>("#retention-menu");
  if (retentionMenu && !element.closest(".retention-select")) setRetentionMenuOpen(false);
  const fixedPositionMenu = document.querySelector<HTMLElement>("#fixed-position-menu");
  if (fixedPositionMenu && !element.closest(".fixed-position-select")) setFixedPositionMenuOpen(false);
  if (!element.closest("#move-group-menu,[data-move-group-trigger]")) closeMoveGroupMenu();
  if (element.closest("#minimize")) { await panel.minimize().catch(error => toast(String(error))); return; }
  if (element.closest("#maximize")) { await panel.toggleMaximize().catch(error => toast(String(error))); return; }
  if (element.closest("#close")) { largePreviewId = null; await action("hide_panel"); return; }
  if (element.closest("#clear-search")) { clearSearch(); return; }
  if (element.closest("#kind-filter")) {
    setFilterMenuOpen(filterMenu.hidden === true);
    return;
  }
  if (element.closest("#clear-filter")) { kindFilter = "all"; savePanelSession(); setFilterMenuOpen(false); renderFilter(); renderList(); return; }
  const kindOption = element.closest<HTMLElement>("[data-kind]");
  if (kindOption) { kindFilter = kindOption.dataset.kind!; savePanelSession(); setFilterMenuOpen(false); renderFilter(); renderList(); return; }
  if (element.closest("#retention-trigger")) { setRetentionMenuOpen(retentionMenu?.hidden === true); return; }
  const retentionOption = element.closest<HTMLButtonElement>("[data-retention]");
  if (retentionOption) {
    document.querySelector<HTMLElement>(".retention-value")!.textContent = retentionOption.textContent;
    document.querySelectorAll<HTMLElement>("[data-retention]").forEach(option => option.setAttribute("aria-selected", String(option === retentionOption)));
    setRetentionMenuOpen(false);
    saveSetting({ retentionDays: Number(retentionOption.dataset.retention) });
    return;
  }
  if (element.closest("#fixed-position-trigger")) { setFixedPositionMenuOpen(fixedPositionMenu?.hidden === true); return; }
  const fixedPositionOption = element.closest<HTMLButtonElement>("[data-fixed-position]");
  if (fixedPositionOption) {
    document.querySelector<HTMLElement>(".fixed-position-value")!.textContent = fixedPositionOption.textContent;
    document.querySelectorAll<HTMLElement>("[data-fixed-position]").forEach(option => option.setAttribute("aria-selected", String(option === fixedPositionOption)));
    setFixedPositionMenuOpen(false);
    saveSetting({ fixedPosition: fixedPositionOption.dataset.fixedPosition as Settings["fixedPosition"] }, true);
    return;
  }
  const moveOption = element.closest<HTMLElement>("[data-move-group]");
  if (moveOption) {
    const ids = moveGroupIds;
    const groupId = moveOption.dataset.moveGroup ? Number(moveOption.dataset.moveGroup) : null;
    closeMoveGroupMenu();
    if (ids.some(id => state.records.find(record => record.id === id)?.groupId !== groupId)) {
      await action("batch_records", { ids, operation: { kind: "move_group", groupId } });
    }
    return;
  }
  const moveTrigger = element.closest<HTMLElement>("[data-move-group-trigger]");
  if (moveTrigger) {
    const menu = document.querySelector<HTMLElement>("#move-group-menu")!;
    const open = menu.hidden || menu.dataset.recordId !== moveTrigger.dataset.moveGroupTrigger;
    closeMoveGroupMenu();
    if (open) openMoveGroupMenu(moveTrigger);
    return;
  }
  if (element.closest("#groups .group.back-to-top")) { scrollBackToTop(); return; }
  const group = element.closest<HTMLElement>("[data-view]");
  if (group) { selectGroup(group.dataset.view!); return; }
  const recordPin = element.closest<HTMLElement>("[data-record-pin]");
  if (recordPin) { const record = state.records.find(record => record.id === recordPin.dataset.recordPin)!; await action("set_record_pinned", { id: record.id, pinned: record.pinnedAt === null }); return; }
  const favorite = element.closest<HTMLElement>("[data-favorite]");
  if (favorite) { const record = state.records.find(record => record.id === favorite.dataset.favorite)!; await action("set_favorite", { id: record.id, favorite: !record.favorite }); return; }
  const deleteButton = element.closest<HTMLElement>("[data-delete]");
  if (deleteButton) { await action("delete_record", { id: deleteButton.dataset.delete }); return; }
  const row = element.closest<HTMLElement>(".record");
  if (row) {
    selectRecord(row.dataset.id!, event.shiftKey); row.focus({ preventScroll: true }); return;
  }
  if (element.closest("#settings-button")) {
    if (settingsOpen) { closeSettings(); return; }
    settingsOpen = true;
    cacheSizeText = "计算中…";
    renderSettings();
    void refreshCacheSize();
    if (!autostartSaving) {
      try { autostartEnabled = await call<boolean>("autostart_enabled"); }
      catch (error) { autostartEnabled = null; toast(String(error)); }
      renderSettings();
    }
    return;
  }
  if (element.closest("#pin")) { await action("save_settings", { value: { ...state.settings, alwaysOnTop: !state.settings.alwaysOnTop } }); return; }
  if (element.closest("#add-group")) { startGroupEdit("create"); return; }
  if (element.id === "clear-cache") {
    if (clearingCache || !window.confirm("清理所有普通记录及其本地文件？收藏和自建分组中的记录会保留。此操作无法撤销。")) return;
    clearingCache = true;
    renderSettings();
    try {
      const count = await call<number>("clear_cache");
      await refresh();
      toast(`已清理 ${count} 条普通记录及孤立附件`);
    } catch (error) {
      await refresh().catch(() => {});
      toast(`清理缓存失败：${String(error)}`);
    } finally {
      clearingCache = false;
      renderSettings();
    }
    return;
  }
});

root.addEventListener("dblclick", event => {
  const group = (event.target as HTMLElement).closest<HTMLElement>("#groups .group[data-view^='group:']");
  if (group && !group.classList.contains("back-to-top")) { startGroupEdit("rename", Number(group.dataset.view!.slice(6))); return; }
  const row = (event.target as HTMLElement).closest<HTMLElement>(".record");
  if (row && !(event.target as HTMLElement).closest("button")) void apply(row.dataset.id!);
});

root.addEventListener("change", event => {
  const element = event.target as HTMLInputElement;
  if (element.id === "autostart") {
    const previous = autostartEnabled;
    const enabled = element.checked;
    autostartEnabled = enabled;
    autostartSaving = true;
    renderSettings();
    void call("set_autostart", { enabled }).catch(error => {
      autostartEnabled = previous;
      toast(String(error));
    }).finally(() => { autostartSaving = false; renderSettings(); });
  }
  if (element.name === "mode") {
    const fixed = element.value === "fixed";
    (document.querySelector("#fixed-position-trigger") as HTMLButtonElement).disabled = !fixed;
    if (!fixed) setFixedPositionMenuOpen(false);
    (document.querySelector("#remember") as HTMLInputElement).disabled = !fixed;
    saveSetting({ positionMode: element.value as Settings["positionMode"] }, true);
  }
  if (element.id === "remember") {
    const enabled = element.checked;
    saveSetting({ rememberPosition: enabled }, !enabled);
  }
  if (element.id === "max-ordinary" || element.id === "sensitive") {
    window.clearTimeout(numberSaveTimers.get(element.id));
    numberSaveTimers.delete(element.id);
    saveNumberSetting(element as HTMLInputElement, true);
  }
});

root.addEventListener("input", event => {
  const element = event.target as HTMLInputElement;
  if (element.id === "search") {
    search = element.value;
    savePanelSession();
    document.querySelector<HTMLButtonElement>("#clear-search")!.hidden = !search;
    renderList();
  }
  if (element.id === "max-ordinary" || element.id === "sensitive") {
    window.clearTimeout(numberSaveTimers.get(element.id));
    numberSaveTimers.set(element.id, window.setTimeout(() => {
      numberSaveTimers.delete(element.id);
      saveNumberSetting(element);
    }, 300));
  }
});

document.addEventListener("keydown", event => {
  if (!event.ctrlKey || event.altKey || event.metaKey || event.shiftKey || event.getModifierState("AltGraph") ||
      event.key.toLowerCase() !== "a") return;
  // 面板本身仍然禁用全选；只有搜索框保留全选，且不依赖 WebView2 的浏览器快捷键开关。
  const target = event.target as HTMLElement;
  if (target instanceof HTMLInputElement && target.id === "search") {
    event.preventDefault();
    target.select();
    return;
  }
  event.preventDefault();
  event.stopImmediatePropagation();
}, true);

root.addEventListener("keydown", event => {
  const element = event.target as HTMLElement;
  if (element.matches("#groups .group-input")) {
    event.stopPropagation();
    if (event.key === "Enter") { event.preventDefault(); void commitGroupEdit(); }
    if (event.key === "Escape") { event.preventDefault(); cancelGroupEdit(); }
    return;
  }
  if (element.id === "shortcut") {
    event.preventDefault();
    if (event.key === "Backspace" || event.key === "Delete") {
      shortcutDraft = "";
      shortcutError = null;
      (element as HTMLInputElement).value = "";
      element.closest("label")?.querySelector(".shortcut-error")?.remove();
      saveSetting({ shortcut: "" });
      return;
    }
    const shortcut = shortcutFromKey(event);
    if (shortcut) {
      shortcutDraft = shortcut;
      shortcutError = null;
      (element as HTMLInputElement).value = displayShortcut(shortcut);
      element.closest("label")?.querySelector(".shortcut-error")?.remove();
      saveSetting({ shortcut });
    }
  }
});

document.addEventListener("keydown", event => {
  if (!state || (event.target as HTMLElement).id === "shortcut") return;
  const recordMenu = document.querySelector<HTMLElement>("#record-menu")!;
  if (!recordMenu.hidden && recordMenu.contains(event.target as Node)) {
    if (event.key === "Escape") {
      event.preventDefault();
      const row = document.querySelector<HTMLElement>(`.record[data-id="${recordMenu.dataset.recordId}"]`);
      closeRecordMenu();
      row?.focus();
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      (event.target as HTMLButtonElement).click();
      return;
    }
    closeRecordMenu();
  }
  if ((event.target as HTMLElement).matches("[data-move-group-trigger]") && (event.key === "Enter" || event.key === " ")) {
    event.preventDefault();
    (event.target as HTMLButtonElement).click();
    return;
  }
  const moveMenu = document.querySelector<HTMLElement>("#move-group-menu")!;
  if (!moveMenu.hidden && moveMenu.contains(event.target as Node)) {
    if (event.key === "Escape") {
      event.preventDefault();
      const trigger = document.querySelector<HTMLElement>(`[data-move-group-trigger="${moveMenu.dataset.recordId}"]`);
      closeMoveGroupMenu();
      trigger?.focus();
    } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const options = [...moveMenu.querySelectorAll<HTMLButtonElement>("button")];
      const index = options.indexOf(event.target as HTMLButtonElement);
      options[(index + (event.key === "ArrowUp" ? -1 : 1) + options.length) % options.length].focus();
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      (event.target as HTMLButtonElement).click();
    } else if (event.key === "Tab") {
      event.preventDefault();
      closeMoveGroupMenu();
      cycleGroup(event.shiftKey ? -1 : 1);
    }
    return;
  }
  if (event.key === "Escape" && !document.querySelector<HTMLElement>("#group-menu")!.hidden) { event.preventDefault(); closeGroupMenu(); return; }
  const filterMenu = document.querySelector<HTMLElement>("#filter-menu")!;
  if (event.key === "Escape" && !filterMenu.hidden) { event.preventDefault(); setFilterMenuOpen(false); return; }
  if (settingsOpen) {
    if (event.key === "Escape") {
      const retentionMenu = document.querySelector<HTMLElement>("#retention-menu");
      const fixedPositionMenu = document.querySelector<HTMLElement>("#fixed-position-menu");
      if (retentionMenu && !retentionMenu.hidden) setRetentionMenuOpen(false);
      else if (fixedPositionMenu && !fixedPositionMenu.hidden) setFixedPositionMenuOpen(false);
      else closeSettings();
    }
    return;
  }
  if (event.shiftKey && event.key === "Escape") { event.preventDefault(); document.querySelector<HTMLInputElement>("#search")!.focus(); return; }
  if (event.isComposing || event.keyCode === 229) {
    document.querySelector<HTMLInputElement>("#search")!.focus();
    return;
  }
  const shortcutDigit = /^Digit([1-9])$/.exec(event.code)?.[1];
  if (event.ctrlKey && !event.getModifierState("AltGraph") && shortcutDigit) {
    event.preventDefault();
    pendingShortcutId = visibleRecords()[Number(shortcutDigit) - 1]?.dataset.id ?? null;
    return;
  }
  if (event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey && !event.getModifierState("AltGraph") &&
      ["KeyD", "KeyF", "KeyG", "KeyP"].includes(event.code)) {
    event.preventDefault();
    if (event.repeat || !selectedId) return;
    const records = filtered().filter(record => selectedIds.has(record.id));
    if (!records.length) return;
    const ids = records.map(record => record.id);
    if (event.code === "KeyD") void action("batch_records", { ids, operation: { kind: "delete" } });
    if (event.code === "KeyF") void action("batch_records", { ids, operation: { kind: "set_favorite", favorite: !records.every(record => record.favorite) } });
    if (event.code === "KeyP") void action("batch_records", { ids, operation: { kind: "set_pinned", pinned: !records.every(record => record.pinnedAt !== null) } });
    if (event.code === "KeyG") {
      const id = selectedId;
      const generation = panelGeneration;
      ensureRecordRow(id);
      window.requestAnimationFrame(() => {
        if (!panelActive || panelGeneration !== generation || selectedId !== id) return;
        const row = recordRow(id);
        const trigger = row?.querySelector<HTMLElement>("[data-move-group-trigger]");
        if (row && trigger) openMoveGroupMenu(trigger, row, records);
      });
    }
    return;
  }
  if (event.key === "Tab") { event.preventDefault(); cycleGroup(event.shiftKey ? -1 : 1); return; }
  if (event.key === "ArrowUp" || event.key === "ArrowDown") {
    event.preventDefault(); document.querySelector<HTMLInputElement>("#search")!.blur();
    moveSelection(event.key === "ArrowUp" ? -1 : 1, event.shiftKey); return;
  }
  // 左右方向键不再切换分组：左键打开大图预览，右键显示悬浮预览。输入类元素内保留原生按键行为。
  if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    if ((event.target as HTMLElement).closest("input, textarea, [contenteditable='true']")) return;
    event.preventDefault();
    if (!selectedId || event.repeat) return;
    const record = state.records.find(item => item.id === selectedId);
    if (event.key === "ArrowLeft") {
      if (record?.kind === "image" || record?.kind === "video") void openLargePreview(selectedId);
    } else if (record) showHoverPreview(selectedId);
    return;
  }
  if (event.code === "Space" && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey &&
      !(event.target as HTMLElement).closest("input, textarea, button, [contenteditable='true']") && selectedId) {
    const record = state.records.find(item => item.id === selectedId);
    if (record?.kind === "image" || record?.kind === "video") {
      event.preventDefault();
      if (!event.repeat) void openLargePreview(selectedId);
      return;
    }
  }
  if (event.key === "Enter" && selectedId) { event.preventDefault(); void apply(selectedId); return; }
  if (event.key === "Escape") {
    // 搜索框有内容时，Esc 先清空搜索词、移走焦点并保留面板；搜索框为空才隐藏面板。
    if (document.querySelector<HTMLInputElement>("#search")!.value !== "") { event.preventDefault(); clearSearch(false); return; }
    largePreviewId = null;
    void action("hide_panel");
    return;
  }
  if (event.key === "Delete" && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey &&
      !(event.target as HTMLElement).closest("input, textarea, [contenteditable='true']")) {
    event.preventDefault();
    if (!event.repeat && selectedIds.size) void action("batch_records", { ids: [...selectedIds], operation: { kind: "delete" } });
    return;
  }
  if ((event.target as HTMLElement).closest("input, textarea, [contenteditable='true']") ||
      event.ctrlKey && !event.getModifierState("AltGraph") || event.metaKey) return;
  const input = document.querySelector<HTMLInputElement>("#search")!;
  if (event.key === "Process" || event.key === "Dead") {
    input.focus();
    return;
  }
  if (event.key === "Backspace") {
    event.preventDefault();
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
    if (input.value) {
      const last = Array.from(input.value).pop()!;
      input.setRangeText("", input.value.length - last.length, input.value.length, "end");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
    return;
  }
  if (Array.from(event.key).length !== 1) return;
  event.preventDefault();
  input.focus();
  input.setRangeText(event.key, input.value.length, input.value.length, "end");
  input.dispatchEvent(new Event("input", { bubbles: true }));
});

document.addEventListener("keyup", event => {
  if (event.key !== "Control" || !pendingShortcutId) return;
  event.preventDefault();
  const id = pendingShortcutId;
  pendingShortcutId = null;
  void apply(id);
});

window.addEventListener("blur", () => {
  pendingShortcutId = null; cancelGroupEdit(); closeGroupMenu(); closeMoveGroupMenu(); closeRecordMenu();
  window.setTimeout(async () => {
    if (!await call<boolean>("preview_focused").catch(() => false)) closePreview();
  }, 120);
});

document.querySelector<HTMLElement>("#list")!.addEventListener("pointerover", event => {
  const row = (event.target as HTMLElement).closest<HTMLElement>(".record");
  if (row?.dataset.id) hoverRecord(row.dataset.id);
});
document.querySelector<HTMLElement>("#list")!.addEventListener("pointerout", event => {
  const row = (event.target as HTMLElement).closest<HTMLElement>(".record");
  if (!row || row.contains(event.relatedTarget as Node)) return;
  if (hoveredId === row.dataset.id) {
    hoveredId = null;
    window.clearTimeout(hoverTimer);
    schedulePreviewClose();
  }
});
document.querySelector<HTMLElement>("#list")!.addEventListener("scroll", () => { renderListRange(); updateOrdinals(); updateBackToTopButton(); closeMoveGroupMenu(); closeRecordMenu(); closePreview(); });
document.querySelector<HTMLElement>("#list")!.addEventListener("wheel", () => { window.cancelAnimationFrame(backToTopFrame); backToTopFrame = 0; });
document.querySelector<HTMLElement>("#list")!.addEventListener("pointerdown", () => { window.cancelAnimationFrame(backToTopFrame); backToTopFrame = 0; });
document.querySelector<HTMLElement>("#list")!.addEventListener("mousedown", event => {
  if (event.button === 0 && event.shiftKey && (event.target as HTMLElement).closest(".record") && !(event.target as HTMLElement).closest("button")) event.preventDefault();
});
window.addEventListener("resize", () => { closeMoveGroupMenu(); closeRecordMenu(); closePreview(); renderListRange(); updateBackToTopTarget(); });

async function start() {
  try {
    await initPreviewWindows();
    await listen<{ id: string; fixed: boolean; width: number; height: number }>("large-preview-loaded", async event => {
      const request = ++largePreviewSizeRequest;
      const { id, fixed, width: imageWidth, height: imageHeight } = event.payload;
      if (largePreviewId !== id) return;
      if (selectedId !== id) {
        largePreviewId = null;
        void emitTo("large-preview", "large-preview-clear");
        return;
      }
      try {
        const [position, size, scale] = await Promise.all([panel.outerPosition(), panel.outerSize(), panel.scaleFactor()]);
        const monitor = await monitorFromPoint(position.x + size.width / 2, position.y + size.height / 2);
        if (largePreviewId !== id || request !== largePreviewSizeRequest) return;
        if (!monitor) throw new Error("无法确定当前屏幕");
        const area = monitor.workArea;
        const maxWidth = Math.max(1, Math.round(Math.min(area.size.width * .9, 1600 * scale)));
        const maxHeight = Math.max(1, Math.round(Math.min(area.size.height * .9, 1000 * scale)));
        const imageScale = fixed ? 1 : Math.min(1, maxWidth / (imageWidth * scale), maxHeight / (imageHeight * scale));
        const width = fixed ? maxWidth : Math.max(1, Math.round(imageWidth * scale * imageScale));
        const height = fixed ? maxHeight : Math.max(1, Math.round(imageHeight * scale * imageScale));
        const x = Math.round(area.position.x + (area.size.width - width) / 2);
        const y = Math.round(area.position.y + (area.size.height - height) / 2);
        if (request !== largePreviewSizeRequest) return;
        await call("show_large_preview", { x, y, width, height });
        if (largePreviewId !== id) await call("hide_large_preview");
      } catch (error) {
        if (request !== largePreviewSizeRequest) return;
        largePreviewId = null;
        void emitTo("large-preview", "large-preview-clear");
        toast(`预览失败：${String(error)}`);
      }
    });
    await listen("large-preview-closed", () => { largePreviewId = null; });
    await listen<{ id: string; generation: number; height: number }>("preview-measured", event => {
      const { id, generation, height: measured } = event.payload;
      const placement = previewPlacement;
      if (id !== previewId || generation !== previewGeneration || !placement || (!hoveredId && !previewHovered)) return;
      const margin = Math.round(12 * placement.scale);
      const height = Math.max(1, Math.min(Math.ceil(measured * placement.scale), placement.bottom - placement.top - 2 * margin));
      const y = Math.max(placement.top + margin, Math.min(placement.rowY, placement.bottom - margin - height));
      const token = previewGeneration;
      void call("show_preview", { x: placement.x, y, width: placement.width, height }).then(() => {
        if (token !== previewGeneration) void call("hide_preview").catch(() => {});
      }).catch(error => toast(`预览失败：${String(error)}`));
    });
    await listen<string>("preview-enter", event => {
      if (event.payload !== previewId) return;
      previewHovered = true;
      window.clearTimeout(previewCloseTimer);
    });
    await listen<string>("preview-leave", event => {
      if (event.payload !== previewId) return;
      previewHovered = false;
      schedulePreviewClose();
    });
    await listen<string>("preview-blur", event => {
      if (event.payload === previewId) closePreview();
    });
    await listen<string>("preview-space", event => {
      if (event.payload === selectedId) void openLargePreview(event.payload);
    });
    await listen("history-changed", () => { void refresh().catch(error => toast(String(error))); });
    await listen("panel-hidden", releasePanelContent);
    await listen("open-requested", () => { void openPanel().catch(error => toast(String(error))); });
    await panel.onMoved(() => { closePreview(); scheduleGeometrySave(); });
    await panel.onResized(() => { closePreview(); scheduleGeometrySave(); });
    ({ view, kindFilter, search } = await call<PanelSession>("get_panel_session"));
    shortcutError = await call<string | null>("panel_ready");
    if (shortcutError) toast(shortcutError);
  } catch (error) {
    toast(`启动失败：${String(error)}`);
  }
}

function scheduleGeometrySave() {
  if (placing || !state || !panelActive) return;
  window.clearTimeout(positionSaveTimer);
  positionSaveTimer = window.setTimeout(async () => {
    const position = await panel.outerPosition(); const size = await panel.innerSize();
    const next = { ...state.settings, panelX: position.x, panelY: position.y,
      panelWidth: size.width, panelHeight: size.height };
    try { await call("save_settings", { value: next }); state.settings = next; } catch { /* keep last valid size */ }
  }, 300);
}

void start();
