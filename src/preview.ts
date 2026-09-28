import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import "./preview.css";

type Kind = "text" | "link" | "image" | "file" | "video" | "color";
type PreviewPayload = {
  id: string; kind: Kind; body: string; files: { original: string }[];
  createdAt: number | null; groupName: string | null; preview: boolean;
  status: string; storageMode: string; width: number;
};

const labels: Record<Kind, string> = { text: "文本", link: "链接", image: "图片", file: "文件", video: "视频", color: "颜色" };
const escapeHtml = (value: string) => value.replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]!);
const root = document.querySelector<HTMLDivElement>("#app")!;
let currentId: string | null = null;
let generation = 0;

function fileName(path: string) { return path.split(/[\\/]/).pop() || path; }

function render(payload: PreviewPayload) {
  const created = payload.createdAt === null ? "" : `<div><dt>创建时间</dt><dd>${escapeHtml(new Date(payload.createdAt).toLocaleString("zh-CN", { hour12: false }))}</dd></div>`;
  const status = payload.status === "pending" ? "保存副本中" : payload.status === "missing" ? "文件已不可用" : payload.status === "copy_failed" ? "副本失败，路径引用" : payload.storageMode === "reference" ? "路径引用" : "";
  const details = `<dl class="details"><div><dt>类型</dt><dd>${labels[payload.kind]}</dd></div><div><dt>分组</dt><dd>${escapeHtml(payload.groupName ?? "未分组")}</dd></div>${created}${status ? `<div><dt>状态</dt><dd>${status}</dd></div>` : ""}</dl>`;
  const paths = payload.files.length ? `<div class="files">${payload.files.map(file => `<div class="file"><strong>${escapeHtml(fileName(file.original))}</strong><div class="path">${escapeHtml(file.original)}</div></div>`).join("")}</div>` : "";
  let content = "";
  if (payload.kind === "text" || payload.kind === "link") content = `<pre class="text-content">${escapeHtml(payload.body)}</pre>`;
  if (payload.kind === "color") {
    const color = /^#(?:[\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8})$/i.test(payload.body.trim()) ? payload.body.trim() : "";
    content = `<div class="color-content"><span class="color-disc" style="background:${color}"></span><span>${escapeHtml(payload.body)}</span></div>`;
  }
  if (payload.kind === "image" || payload.kind === "video") content = payload.kind === "image" || payload.preview ? `<div class="media"><img id="large-preview" alt="${labels[payload.kind]}预览"></div>` : `<p class="unavailable">首帧预览不可用</p>`;
  root.innerHTML = `<div class="card" style="width:${payload.width}px">${details}${content}${paths}</div>`;
}

async function showContent(payload: PreviewPayload) {
  currentId = payload.id;
  const token = ++generation;
  render(payload);
  const image = document.querySelector<HTMLImageElement>("#large-preview");
  if (image) {
    try {
      image.src = await invoke<string>("preview_data", { id: payload.id, full: payload.kind === "image" });
      await image.decode();
    } catch {
      image.closest(".media")!.innerHTML = `<p class="unavailable">预览图不可用</p>`;
    }
  }
  if (token !== generation) return;
  await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
  await emitTo("main", "preview-measured", { id: payload.id, height: root.querySelector<HTMLElement>(".card")!.offsetHeight });
}

document.body.addEventListener("pointerenter", () => { if (currentId) void emitTo("main", "preview-enter", currentId); });
document.body.addEventListener("pointerleave", () => { if (currentId) void emitTo("main", "preview-leave", currentId); });
window.addEventListener("blur", () => { if (currentId) void emitTo("main", "preview-blur", currentId); });

async function start() {
  await listen<PreviewPayload>("preview-content", event => { void showContent(event.payload); });
  await listen("preview-ping", () => { void emitTo("main", "preview-ready"); });
  await emitTo("main", "preview-ready");
}

void start();
