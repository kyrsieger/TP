import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./large-preview.css";

type Payload = { id: string; kind: "image" | "video" };

const image = document.querySelector<HTMLImageElement>("#image")!;
const error = document.querySelector<HTMLElement>("#error")!;
const largeWindow = getCurrentWindow();
image.draggable = false;
let currentId: string | null = null;
let generation = 0;

function clear() {
  generation++;
  currentId = null;
  image.removeAttribute("src");
}

async function close() {
  if (!currentId) return;
  clear();
  await invoke("hide_large_preview");
}

async function show(payload: Payload) {
  const token = ++generation;
  currentId = payload.id;
  image.hidden = false;
  error.hidden = true;
  image.removeAttribute("src");
  image.alt = payload.kind === "video" ? "视频首帧预览" : "图片预览";
  document.querySelector<HTMLElement>("#kind")!.textContent = payload.kind === "video" ? "视频首帧" : "";
  try {
    const src = await invoke<string>("preview_data", { id: payload.id, role: payload.kind === "image" ? "full" : "preview" });
    if (token !== generation) return;
    image.src = src;
    await image.decode();
  } catch {
    if (token !== generation) return;
    image.hidden = true;
    error.hidden = false;
  }
  if (token === generation) await emitTo("main", "large-preview-loaded", {
    id: payload.id,
    width: image.hidden ? 320 : image.naturalWidth,
    height: image.hidden ? 180 : image.naturalHeight,
  });
}

document.addEventListener("keydown", event => {
  if (event.key === "Escape" || event.key === " ") {
    event.preventDefault();
    void close();
  }
});
document.querySelector<HTMLElement>("#close")!.addEventListener("click", () => { void close(); });
document.querySelector<HTMLElement>("#app")!.addEventListener("pointerdown", event => {
  if (event.button === 0 && !(event.target as HTMLElement).closest("#close")) void largeWindow.startDragging();
});

async function start() {
  await listen<Payload>("large-preview-content", event => { void show(event.payload); });
  await listen("large-preview-clear", clear);
  await listen("large-preview-ping", () => { void emitTo("main", "large-preview-ready"); });
  await emitTo("main", "large-preview-ready");
}

void start();
