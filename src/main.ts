import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { cursorPosition, currentMonitor, getCurrentWindow, PhysicalSize, monitorFromPoint, PhysicalPosition } from "@tauri-apps/api/window";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import "./styles.css";

type Kind = "text" | "link" | "image" | "file" | "video" | "color";
type FileItem = { original: string; stored: string | null };
type RecordItem = {
  id: string; kind: Kind; body: string; files: FileItem[]; favorite: boolean;
  groupId: number | null; usedAt: number; expiresAt: number | null;
  storageMode: string; status: string; preview: boolean;
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

const panel = getCurrentWindow();
const isMacOS = navigator.userAgent.includes("Macintosh");
const root = document.querySelector<HTMLDivElement>("#app")!;
let state: Snapshot;
let view = "default";
let kindFilter = "all";
let search = "";
let selectedId: string | null = null;
let pendingShortcutId: string | null = null;
let settingsOpen = false;
let clearingCache = false;
let activeShortcut = "";
let positionSaveTimer: number | undefined;
const numberSaveTimers = new Map<string, number>();
let settingsSave: Promise<void> = Promise.resolve();
let placing = false;
let lastResizePress: { edge: string; time: number; x: number; y: number } | null = null;

const labels: Record<Kind, string> = { text: "文本", link: "链接", image: "图片", file: "文件", video: "视频", color: "颜色" };
const icons: Record<Kind, string> = { text: "文", link: "链", image: "图", file: "件", video: "影", color: "色" };
const escapeHtml = (value: string) => value.replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]!);
const call = async <T>(name: string, args: Record<string, unknown> = {}): Promise<T> => invoke<T>(name, args);

function toast(message: string) {
  const el = document.querySelector<HTMLElement>("#toast")!;
  el.textContent = message;
  el.classList.add("visible");
  window.setTimeout(() => el.classList.remove("visible"), 3500);
}

async function action(name: string, args: Record<string, unknown> = {}) {
  try { await call(name, args); await refresh(); }
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
  const tabs = document.querySelector<HTMLElement>("#groups")!;
  const groups = [{ id: "default", name: "默认" }, { id: "favorite", name: "收藏" },
    ...state.groups.map(group => ({ id: `group:${group.id}`, name: group.name }))];
  tabs.innerHTML = groups.map(group => `<button class="group ${view === group.id ? "active" : ""}" data-view="${group.id}">${escapeHtml(group.name)}</button>`).join("")
    + `<button class="add-group" title="添加分组" id="add-group">＋</button>`;
  document.querySelector<HTMLElement>("#filter-wrap")!.hidden = view !== "default";
  const name = groups.find(group => group.id === view)?.name ?? "默认";
  document.querySelector<HTMLElement>("#view-title")!.textContent = name;
  document.querySelector<HTMLElement>("#delete-group")!.hidden = !view.startsWith("group:");
}

function recordTitle(record: RecordItem): string {
  if (record.kind === "image") return "图片";
  if (record.files.length > 1) return `${record.files.length} 个文件`;
  if (record.files.length === 1) return record.files[0].original.split(/[\\/]/).pop() || "文件";
  return record.body.split("\n")[0] || "空文本";
}

function recordDetail(record: RecordItem): string {
  if (record.files.length) return record.files.map(file => file.original).join(" · ");
  if (record.kind === "image") return "点击两次放回剪贴板";
  return record.body;
}

function renderList() {
  const list = document.querySelector<HTMLElement>("#list")!;
  const scrollTop = list.scrollTop;
  const records = filtered();
  document.querySelector<HTMLElement>("#count")!.textContent = `${records.length} 条`;
  list.innerHTML = records.length ? records.map(record => {
    const status = record.status === "pending" ? "保存副本中" : record.status === "missing" ? "文件已不可用" : record.status === "copy_failed" ? "副本保存失败，使用路径引用" : record.storageMode === "reference" ? "路径引用" : record.storageMode === "copy" ? "已保存副本" : "";
    const isText = record.kind === "text" || record.kind === "link" || record.kind === "color";
    const swatch = record.kind === "color" && /^#(?:[\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8})$/i.test(record.body.trim());
    return `<article class="record ${selectedId === record.id ? "selected" : ""}" data-id="${record.id}" tabindex="-1">
      <span class="ordinal"></span><span class="kind-icon ${swatch ? "color-swatch" : ""}" ${swatch ? `style="background-color:${record.body.trim()}" title="颜色"` : ""}>${swatch ? "" : icons[record.kind]}</span>
      <div class="record-main">${isText ? "" : `<div class="record-heading"><strong>${escapeHtml(recordTitle(record))}</strong><span class="kind-label">${labels[record.kind]}</span></div>`}
      ${record.preview ? `<img class="preview" data-preview="${record.id}" alt="${labels[record.kind]}预览" />${record.kind === "video" ? `<span class="play">▶</span>` : ""}` : ""}
      <p${isText ? ' class="record-text"' : ""}>${escapeHtml(recordDetail(record))}</p><small>${status || (record.expiresAt ? "敏感记录" : new Date(record.usedAt).toLocaleString())}</small></div>
      <div class="record-actions"><button class="favorite ${record.favorite ? "on" : ""}" data-favorite="${record.id}" title="收藏">${record.favorite ? "★" : "☆"}</button>
      <select data-group="${record.id}" aria-label="移动到分组"><option value="">未分组</option>${state.groups.map(group => `<option value="${group.id}" ${record.groupId === group.id ? "selected" : ""}>${escapeHtml(group.name)}</option>`).join("")}</select>
      <button class="delete" data-delete="${record.id}" title="删除记录">×</button></div></article>`;
  }).join("") : `<div class="empty">这里还没有记录</div>`;
  list.scrollTop = scrollTop;
  updateOrdinals();
  list.querySelectorAll<HTMLImageElement>("[data-preview]").forEach(async image => {
    try { image.src = await call<string>("preview_data", { id: image.dataset.preview }); } catch { image.remove(); }
  });
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
  visibleRecords().forEach((row, index) => row.querySelector<HTMLElement>(".ordinal")!.textContent = String(index + 1));
}

function renderSettings() {
  const settings = document.querySelector<HTMLElement>("#settings")!;
  settings.hidden = !settingsOpen;
  if (!settingsOpen) return;
  const s = state.settings;
  settings.innerHTML = `<h2>设置</h2><label>唤出快捷键<input id="shortcut" placeholder="点击后按组合键" value="${escapeHtml(s.shortcut)}" readonly></label>
    <label>普通记录数量上限<input id="max-ordinary" type="number" min="1" max="500" value="${s.maxOrdinary}"></label>
    <label>普通记录保留时间<select id="retention">${[[1,"24 小时"],[3,"3 天"],[5,"5 天"],[7,"7 天"],[30,"1 个月"]].map(([value,label]) => `<option value="${value}" ${s.retentionDays === value ? "selected" : ""}>${label}</option>`).join("")}</select></label>
    <label>1Password 记录保留秒数<input id="sensitive" type="number" min="1" max="4294967295" value="${s.sensitiveSeconds}"></label>
    <p class="hint">数量和时间上限只作用于普通记录。收藏或已分组的记录只能手动删除。</p>
    <fieldset><legend>面板位置</legend><label class="inline"><input type="radio" name="mode" value="fixed" ${s.positionMode === "fixed" ? "checked" : ""}>固定位置</label>
    <label class="inline"><input type="radio" name="mode" value="cursor" ${s.positionMode === "cursor" ? "checked" : ""}>跟随光标</label></fieldset>
    <label>固定位置<select id="fixed-position" ${s.positionMode === "cursor" ? "disabled" : ""}><option value="left" ${s.fixedPosition === "left" ? "selected" : ""}>左下角</option><option value="right" ${s.fixedPosition === "right" ? "selected" : ""}>右下角</option><option value="center" ${s.fixedPosition === "center" ? "selected" : ""}>下方居中</option></select></label>
    <label class="inline"><input id="remember" type="checkbox" ${s.rememberPosition ? "checked" : ""} ${s.positionMode === "cursor" ? "disabled" : ""}>记忆上次拖拽位置</label>
    <p class="hint">拖拽面板边缘调整大小；缩放光标出现时，双击左右边缘恢复默认宽度，双击上下边缘恢复默认高度，双击角落恢复两维。大小会在下次打开时恢复。</p>
    <p class="hint">清理缓存会删除所有普通记录、对应的本地文件及孤立附件。收藏和自建分组中的记录会保留。</p>
    <button id="clear-cache" class="primary" ${clearingCache ? "disabled" : ""}>${clearingCache ? "清理中…" : "清理缓存"}</button>`;
}

function render() {
  renderGroups(); renderList(); renderSettings();
  const pin = document.querySelector<HTMLButtonElement>("#pin")!;
  pin.setAttribute("aria-pressed", String(state.settings.alwaysOnTop));
  pin.title = state.settings.alwaysOnTop ? "取消置顶" : "置顶";
  pin.setAttribute("aria-label", pin.title);
}
async function refresh() { state = await call<Snapshot>("get_state"); render(); }

async function apply(id: string) {
  try { toast(await call<string>("apply_record", { id })); await refresh(); }
  catch (error) { toast(String(error)); }
}

function cycleGroup(direction: number) {
  const views = ["default", "favorite", ...state.groups.map(group => `group:${group.id}`)];
  view = views[(views.indexOf(view) + direction + views.length) % views.length];
  selectedId = null; render();
}

function moveSelection(direction: number) {
  const records = filtered();
  if (!records.length) return;
  const index = records.findIndex(record => record.id === selectedId);
  selectedId = records[Math.max(0, Math.min(records.length - 1, index + direction))].id;
  const list = document.querySelector<HTMLElement>("#list")!;
  list.querySelector<HTMLElement>(".record.selected")?.classList.remove("selected");
  const row = [...list.querySelectorAll<HTMLElement>(".record")].find(row => row.dataset.id === selectedId);
  row?.classList.add("selected");
  row?.scrollIntoView({ block: "nearest" });
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
  if (shortcut) await register(shortcut, () => { void openPanel().catch(error => toast(String(error))); });
  if (activeShortcut) await unregister(activeShortcut);
  activeShortcut = shortcut;
}

function saveSetting(patch: Partial<Settings>, reposition = false) {
  settingsSave = settingsSave.then(async () => {
    const previous = state.settings;
    const next = { ...previous, ...patch };
    if (Object.keys(patch).every(key => previous[key as keyof Settings] === next[key as keyof Settings])) return;
    try {
      if (patch.shortcut !== undefined) await registerShortcut(next.shortcut);
      await call("save_settings", { value: next });
      state.settings = next;
    } catch (error) {
      if (patch.shortcut !== undefined) await registerShortcut(previous.shortcut).catch(() => {});
      renderSettings();
      toast(String(error));
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

async function openPanel() {
  await call("remember_paste_target");
  await placePanel();
  await call("show_panel");
}

function shortcutFromKey(event: KeyboardEvent): string | null {
  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
  if (["Control", "Alt", "Shift", "Meta"].includes(key)) return null;
  const modifiers = [event.ctrlKey && "Control", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Super"].filter(Boolean);
  if (!modifiers.length || !/^([A-Z0-9]|F\d{1,2})$/.test(key)) return null;
  return [...modifiers, key].join("+");
}

root.innerHTML = `<div class="shell"><div class="resize-edge left" data-resize-edge="left"></div><div class="resize-edge right" data-resize-edge="right"></div><div class="resize-edge top" data-resize-edge="top"></div><div class="resize-edge bottom" data-resize-edge="bottom"></div>
  <div class="toolbar"><input id="search" placeholder="搜索当前分组 · Shift+Esc" aria-label="搜索当前分组"><button id="pin" aria-label="置顶" aria-pressed="false" title="置顶"><svg aria-hidden="true" viewBox="0 0 24 24" fill="currentColor"><path d="M16 9V4l1-1V2H7v1l1 1v5c0 1.66-1.34 3-3 3v2h6v7l1 1 1-1v-7h6v-2c-1.66 0-3-1.34-3-3z"/></svg></button><button id="settings-button" aria-label="设置" title="设置">⚙</button></div>
  <nav id="groups"></nav><div class="section-heading"><strong id="view-title">默认</strong><span id="count"></span><button id="delete-group" hidden>删除分组</button>
  <label id="filter-wrap">类型 <select id="kind-filter"><option value="all">全部</option><option value="text">文本</option><option value="image">图片</option><option value="video">视频</option><option value="file">文件</option><option value="link">链接</option><option value="color">颜色</option></select></label></div>
  <main id="list"></main><section id="settings" hidden></section><footer>双击应用 · Ctrl + 1–9 选取可见记录</footer><div id="toast" role="status"></div></div>`;

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

root.addEventListener("click", async event => {
  const element = event.target as HTMLElement;
  const group = element.closest<HTMLElement>("[data-view]");
  if (group) { view = group.dataset.view!; selectedId = null; render(); return; }
  const favorite = element.closest<HTMLElement>("[data-favorite]");
  if (favorite) { const record = state.records.find(record => record.id === favorite.dataset.favorite)!; await action("set_favorite", { id: record.id, favorite: !record.favorite }); return; }
  const deleteButton = element.closest<HTMLElement>("[data-delete]");
  if (deleteButton) { await action("delete_record", { id: deleteButton.dataset.delete }); return; }
  const row = element.closest<HTMLElement>(".record");
  if (row && !element.closest("select")) {
    document.querySelector<HTMLElement>(".record.selected")?.classList.remove("selected");
    selectedId = row.dataset.id!; row.classList.add("selected"); return;
  }
  if (element.id === "settings-button") { settingsOpen = !settingsOpen; renderSettings(); return; }
  if (element.closest("#pin")) { await action("save_settings", { value: { ...state.settings, alwaysOnTop: !state.settings.alwaysOnTop } }); return; }
  if (element.id === "add-group") {
    const name = window.prompt("新分组名称"); if (name) await action("add_group", { name }); return;
  }
  if (element.id === "delete-group" && view.startsWith("group:")) {
    if (window.confirm("删除这个分组？其中的记录会回到未分组状态。")) {
      await action("delete_group", { id: Number(view.slice(6)) }); view = "default"; render();
    }
    return;
  }
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
  const row = (event.target as HTMLElement).closest<HTMLElement>(".record");
  if (row && !(event.target as HTMLElement).closest("button,select")) void apply(row.dataset.id!);
});

root.addEventListener("change", event => {
  const element = event.target as HTMLInputElement | HTMLSelectElement;
  if (element.id === "kind-filter") { kindFilter = element.value; renderList(); }
  if (element.dataset.group) void action("move_group", { id: element.dataset.group, groupId: element.value ? Number(element.value) : null });
  if (element.name === "mode") {
    const fixed = element.value === "fixed";
    (document.querySelector("#fixed-position") as HTMLSelectElement).disabled = !fixed;
    (document.querySelector("#remember") as HTMLInputElement).disabled = !fixed;
    saveSetting({ positionMode: element.value as Settings["positionMode"] }, true);
  }
  if (element.id === "retention") saveSetting({ retentionDays: Number(element.value) });
  if (element.id === "fixed-position") saveSetting({ fixedPosition: element.value as Settings["fixedPosition"] }, true);
  if (element.id === "remember") saveSetting({ rememberPosition: (element as HTMLInputElement).checked }, true);
  if (element.id === "max-ordinary" || element.id === "sensitive") {
    window.clearTimeout(numberSaveTimers.get(element.id));
    numberSaveTimers.delete(element.id);
    saveNumberSetting(element as HTMLInputElement, true);
  }
});

root.addEventListener("input", event => {
  const element = event.target as HTMLInputElement;
  if (element.id === "search") { search = element.value; renderList(); }
  if (element.id === "max-ordinary" || element.id === "sensitive") {
    window.clearTimeout(numberSaveTimers.get(element.id));
    numberSaveTimers.set(element.id, window.setTimeout(() => {
      numberSaveTimers.delete(element.id);
      saveNumberSetting(element);
    }, 300));
  }
});

root.addEventListener("keydown", event => {
  const element = event.target as HTMLElement;
  if (element.id === "shortcut") {
    event.preventDefault();
    if (event.key === "Backspace" || event.key === "Delete") {
      (element as HTMLInputElement).value = "";
      saveSetting({ shortcut: "" });
      return;
    }
    const shortcut = shortcutFromKey(event);
    if (shortcut) { (element as HTMLInputElement).value = shortcut; saveSetting({ shortcut }); }
  }
});

document.addEventListener("keydown", event => {
  if (!state || (event.target as HTMLElement).id === "shortcut") return;
  if (settingsOpen) {
    if (event.key === "Escape") { settingsOpen = false; renderSettings(); }
    return;
  }
  if (event.shiftKey && event.key === "Escape") { event.preventDefault(); document.querySelector<HTMLInputElement>("#search")!.focus(); return; }
  const shortcutDigit = /^Digit([1-9])$/.exec(event.code)?.[1];
  if (event.ctrlKey && shortcutDigit) {
    event.preventDefault();
    pendingShortcutId = visibleRecords()[Number(shortcutDigit) - 1]?.dataset.id ?? null;
    return;
  }
  if (event.key === "Tab" || event.key === "ArrowLeft" || event.key === "ArrowRight") {
    event.preventDefault(); cycleGroup(event.key === "ArrowLeft" || (event.key === "Tab" && event.shiftKey) ? -1 : 1); return;
  }
  if (event.key === "ArrowUp" || event.key === "ArrowDown") {
    event.preventDefault(); moveSelection(event.key === "ArrowUp" ? -1 : 1); return;
  }
  if (event.key === "Enter" && selectedId) { event.preventDefault(); void apply(selectedId); return; }
  if (event.key === "Escape") { void action("hide_panel"); }
});

document.addEventListener("keyup", event => {
  if (event.key !== "Control" || !pendingShortcutId) return;
  event.preventDefault();
  const id = pendingShortcutId;
  pendingShortcutId = null;
  void apply(id);
});

window.addEventListener("blur", () => pendingShortcutId = null);

document.querySelector<HTMLElement>("#list")!.addEventListener("scroll", updateOrdinals);

async function start() {
  try {
    await refresh(); await placePanel(); await registerShortcut(state.settings.shortcut);
    await listen("history-changed", () => { void refresh(); });
    await listen("open-requested", () => { void openPanel().catch(error => toast(String(error))); });
    await panel.onMoved(() => scheduleGeometrySave());
    await panel.onResized(() => scheduleGeometrySave());
  } catch (error) { toast(`启动失败：${String(error)}`); }
}

function scheduleGeometrySave() {
  if (placing || !state) return;
  window.clearTimeout(positionSaveTimer);
  positionSaveTimer = window.setTimeout(async () => {
    const position = await panel.outerPosition(); const size = await panel.innerSize();
    const next = { ...state.settings, panelX: position.x, panelY: position.y,
      panelWidth: size.width, panelHeight: size.height };
    try { await call("save_settings", { value: next }); state.settings = next; } catch { /* keep last valid size */ }
  }, 300);
}

void start();
