import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { indicatorHtml } from "./media-indicators";
import { createMediaCarousel } from "./media-carousel";
import "./preview.css";
import "./media-indicators.css";

type Kind = "text" | "link" | "image" | "file" | "video" | "color";
type PreviewPayload = {
  id: string; kind: Kind; body: string; files: { original: string }[];
  createdAt: number | null; groupName: string | null; preview: boolean;
  status: string; width: number; generation: number;
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

function render(payload: PreviewPayload) {
  const created = payload.createdAt === null ? "" : `<div><dt>创建时间</dt><dd>${escapeHtml(new Date(payload.createdAt).toLocaleString("zh-CN", { hour12: false }))}</dd></div>`;
  const group = payload.groupName === null ? "" : `<div><dt>分组</dt><dd>${escapeHtml(payload.groupName)}</dd></div>`;
  const status = payload.status === "pending" ? "保存副本中" : payload.status === "missing" ? "文件已不可用" : payload.status === "copy_failed" ? "副本保存失败" : "";
  const details = `<dl class="details"><div><dt>类型</dt><dd>${labels[payload.kind]}</dd></div>${group}${created}${status ? `<div><dt>状态</dt><dd>${status}</dd></div>` : ""}</dl>`;
  const paths = payload.files.length ? `<ul class="files">${payload.files.map(file => `<li class="file"><strong>${escapeHtml(fileName(file.original))}</strong><div class="path">${escapeHtml(file.original)}</div></li>`).join("")}</ul>` : "";
  let content = "";
  if (payload.kind === "text" || payload.kind === "link") content = `<pre class="text-content">${escapeHtml(payload.body)}</pre>`;
  if (payload.kind === "color") {
    const color = /^#(?:[\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8})$/i.test(payload.body.trim()) ? payload.body.trim() : "";
    content = `<div class="color-content"><span class="color-disc" style="background:${color}"></span><span>${escapeHtml(payload.body)}</span></div>`;
  }
  if (payload.kind === "image" || payload.kind === "video") content = payload.files.length > 1
    ? `<div class="media multi"><div class="media-stage"></div>${indicatorHtml(payload.files.length)}</div>`
    : `<div class="media"><img id="large-preview" alt="${labels[payload.kind]}预览" hidden><p id="media-error" class="unavailable" hidden></p></div>`;
  const media = payload.kind === "image" || payload.kind === "video";
  root.innerHTML = `<div class="card" style="width:${payload.width}px">${media ? content + details : details + content}${paths}</div>`;
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
  else await measure(payload, token);
}

document.body.addEventListener("pointerenter", () => { if (currentId) void emitTo("main", "preview-enter", currentId); });
document.body.addEventListener("pointerleave", () => { if (currentId) void emitTo("main", "preview-leave", currentId); });
window.addEventListener("blur", () => { if (currentId) void emitTo("main", "preview-blur", currentId); });
document.addEventListener("keydown", event => {
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
