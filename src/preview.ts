import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { indicatorHtml } from "./media-indicators";
import { createMediaCarousel } from "./media-carousel";
import { highlightSearch } from "./search-highlight";
import "./preview.css";
import "./media-indicators.css";

type Kind = "text" | "link" | "image" | "file" | "video" | "color";
type PreviewPayload = {
  id: string; kind: Kind; body: string; files: { original: string }[];
  groupName: string | null; preview: boolean;
  status: string; width: number; maxHeight: number; generation: number; search: string;
};

const labels: Record<Kind, string> = { text: "文本", link: "链接", image: "图片", file: "文件", video: "视频", color: "颜色" };
const escapeHtml = (value: string) => value.replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]!);
const root = document.querySelector<HTMLDivElement>("#app")!;
let currentId: string | null = null;
let currentKind: Kind | null = null;
let currentRequest: number | null = null;
let generation = 0;
let carousel: ReturnType<typeof createMediaCarousel> | null = null;
let lastWheelAt = 0;

function fileName(path: string) { return path.split(/[\\/]/).pop() || path; }
function pathHtml(path: string, query: string) {
  const name = fileName(path);
  return `${highlightSearch(path.slice(0, path.length - name.length), query)}<strong>${highlightSearch(name, query)}</strong>`;
}

function render(payload: PreviewPayload) {
  const group = payload.groupName === null ? "" : `<div><dt>分组</dt><dd>${escapeHtml(payload.groupName)}</dd></div>`;
  const status = payload.status === "missing" ? "文件已不可用" : payload.status === "copy_failed" ? "副本保存失败" : "";
  const details = group ? `<dl class="details">${group}</dl>` : "";
  const statusNote = status ? `<div class="status-note">${status}</div>` : "";
  const paths = payload.files.length ? `<ul class="files${payload.files.length === 1 ? " single-file" : ""}">${payload.files.map((file, index) => `<li class="file" data-index="${index}"><div class="path">${pathHtml(file.original, payload.search)}</div></li>`).join("")}</ul>` : "";
  let content = "";
  if (payload.kind === "text" || payload.kind === "link") content = `<pre class="text-content">${highlightSearch(payload.body, payload.search)}</pre>`;
  if (payload.kind === "color") {
    const color = /^#(?:[\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8})$/i.test(payload.body.trim()) ? payload.body.trim() : "";
    content = `<div class="color-content"><span class="color-disc" style="background:${color}"></span><span>${highlightSearch(payload.body, payload.search)}</span></div>`;
  }
  if (payload.kind === "image" || payload.kind === "video") content = payload.files.length > 1
    ? `<div class="media multi"><div class="media-stage"></div>${indicatorHtml(payload.files.length)}</div>`
    : `<div class="media"><img id="large-preview" alt="${labels[payload.kind]}预览" hidden><p id="media-error" class="unavailable" hidden></p></div>`;
  const text = payload.kind === "text";
  // 正文在最上，其后依次是字段、原始路径，statusNote 固定收尾。
  root.innerHTML = `<div class="card${text ? " text-card" : ""}" style="width:${payload.width}px;${text ? `max-height:${payload.maxHeight}px` : ""}">${content}${details}${paths}${statusNote}</div>`;
  const textArea = root.querySelector<HTMLElement>(".text-content");
  const match = textArea?.querySelector<HTMLElement>(".search-hit");
  if (textArea && match) textArea.scrollTop = match.getBoundingClientRect().top - textArea.getBoundingClientRect().top - textArea.clientHeight / 2;
}

async function measure(payload: PreviewPayload, token: number) {
  if (token !== generation) return;
  await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
  if (token !== generation) return;
  await emitTo("main", "preview-measured", { id: payload.id, height: root.querySelector<HTMLElement>(".card")!.offsetHeight });
}

async function loadSingle(payload: PreviewPayload, token: number) {
  const image = document.querySelector<HTMLImageElement>("#large-preview");
  const error = document.querySelector<HTMLElement>("#media-error");
  if (!image || !error) return;
  image.hidden = true;
  image.removeAttribute("src");
  error.hidden = true;
  try {
    const src = await invoke<string>("preview_data", { id: payload.id, role: "preview", index: 0 });
    if (token !== generation) return;
    image.src = src;
    await image.decode();
    if (token !== generation) return;
    image.hidden = false;
  } catch {
    if (token !== generation) return;
    error.textContent = payload.kind === "video" ? "首帧预览不可用" : "预览图不可用";
    error.hidden = false;
  }
  await measure(payload, token);
}

async function loadFileText(payload: PreviewPayload, token: number) {
  await Promise.all(payload.files.map(async (_, index) => {
    let content: string | null = null;
    let error = "";
    try {
      content = await invoke<string | null>("file_text_preview", { id: payload.id, index });
    } catch (reason) {
      error = String(reason);
    }
    if (token !== generation || (content === null && !error)) return;
    const file = root.querySelector<HTMLElement>(`.file[data-index="${index}"]`);
    if (!file) return;
    const preview = document.createElement(content === null ? "p" : "pre");
    preview.className = content === null ? "file-text-error" : "file-text-content";
    preview.textContent = content ?? `文本预览不可用：${error}`;
    file.append(preview);
  }));
  await measure(payload, token);
}

async function showContent(payload: PreviewPayload) {
  carousel?.destroy();
  carousel = null;
  const token = ++generation;
  currentId = payload.id;
  currentKind = payload.kind;
  currentRequest = payload.generation;
  lastWheelAt = 0;
  render(payload);
  if ((payload.kind === "image" || payload.kind === "video") && payload.files.length > 1) {
    carousel = createMediaCarousel(
      document.querySelector<HTMLElement>(".media-stage")!,
      document.querySelector<HTMLElement>(".page-indicators")!,
      payload.files.length,
      index => invoke<string>("preview_data", { id: payload.id, role: "preview", index }),
      payload.kind === "video" ? "首帧预览不可用" : "预览图不可用",
      "frame",
    );
    await carousel.start();
    await measure(payload, token);
  } else if (payload.kind === "image" || payload.kind === "video") await loadSingle(payload, token);
  else {
    await measure(payload, token);
    if (payload.kind === "file") void loadFileText(payload, token);
  }
}

document.body.addEventListener("pointerenter", () => { if (currentId) void emitTo("main", "preview-enter", currentId); });
document.body.addEventListener("pointerleave", () => { if (currentId) void emitTo("main", "preview-leave", currentId); });
window.addEventListener("blur", () => { if (currentId) void emitTo("main", "preview-blur", currentId); });
document.addEventListener("contextmenu", event => {
  if (currentKind === "text" || currentKind === "link" || currentKind === "color") event.preventDefault();
});
document.addEventListener("keydown", event => {
  if (event.ctrlKey && event.key.toLowerCase() === "a" && !event.altKey && !event.metaKey && !event.shiftKey) {
    event.preventDefault();
    return;
  }
  if (event.code === "Space" && currentId && (currentKind === "image" || currentKind === "video")) {
    event.preventDefault();
    if (!event.repeat) void emitTo("main", "preview-space", currentId);
  }
});
document.addEventListener("wheel", event => {
  if (!carousel || !(event.target as HTMLElement).closest(".media.multi") || !event.deltaY) return;
  event.preventDefault();
  const now = performance.now();
  if (now - lastWheelAt < 180) return;
  lastWheelAt = now;
  carousel.step(Math.sign(event.deltaY));
}, { passive: false });

async function start() {
  await listen<PreviewPayload>("preview-content", event => { void showContent(event.payload); });
  await listen<number>("preview-clear", event => {
    if (event.payload !== currentRequest) return;
    generation++;
    currentId = null;
    currentKind = null;
    currentRequest = null;
    carousel?.destroy();
    carousel = null;
    document.querySelector<HTMLImageElement>("#large-preview")?.removeAttribute("src");
    root.replaceChildren();
  });
  await listen("preview-ping", () => { void emitTo("main", "preview-ready"); });
  await emitTo("main", "preview-ready");
}

void start();
