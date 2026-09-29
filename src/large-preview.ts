import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { indicatorHtml } from "./media-indicators";
import { createMediaCarousel } from "./media-carousel";
import "./large-preview.css";
import "./media-indicators.css";

type Payload = { id: string; kind: "image" | "video"; count: number };

const image = document.querySelector<HTMLImageElement>("#image")!;
const error = document.querySelector<HTMLElement>("#error")!;
const stage = document.querySelector<HTMLElement>("#stage")!;
const largeWindow = getCurrentWindow();
image.draggable = false;
let currentId: string | null = null;
let carousel: ReturnType<typeof createMediaCarousel> | null = null;
let lastWheelAt = 0;
let generation = 0;

function clear() {
  generation++;
  currentId = null;
  carousel?.destroy();
  carousel = null;
  image.removeAttribute("src");
}

async function close() {
  if (!currentId) return;
  clear();
  await invoke("hide_large_preview");
}

async function show(payload: Payload) {
  carousel?.destroy();
  carousel = null;
  const token = ++generation;
  currentId = payload.id;
  lastWheelAt = 0;
  image.alt = payload.kind === "video" ? "视频首帧预览" : "图片预览";
  document.querySelector<HTMLElement>("#kind")!.textContent = payload.kind === "video" ? "视频首帧" : "";
  const indicators = document.querySelector<HTMLElement>("#indicators")!;
  indicators.innerHTML = indicatorHtml(payload.count);
  image.hidden = true;
  error.hidden = true;
  image.removeAttribute("src");
  if (payload.count > 1) {
    stage.hidden = false;
    carousel = createMediaCarousel(
      stage,
      indicators.querySelector<HTMLElement>(".page-indicators")!,
      payload.count,
      index => invoke<string>("preview_data", { id: payload.id, role: payload.kind === "image" ? "full" : "preview", index }),
      payload.kind === "video" ? "首帧预览不可用" : "预览图不可用",
    );
    await carousel.start();
    if (token === generation) await emitTo("main", "large-preview-loaded", { id: payload.id, fixed: true, width: 0, height: 0 });
    return;
  }
  stage.hidden = true;
  try {
    const src = await invoke<string>("preview_data", { id: payload.id, role: payload.kind === "image" ? "full" : "preview", index: 0 });
    if (token !== generation) return;
    image.src = src;
    await image.decode();
    if (token !== generation) return;
    image.hidden = false;
  } catch {
    if (token !== generation) return;
    error.hidden = false;
  }
  await emitTo("main", "large-preview-loaded", {
    id: payload.id,
    fixed: false,
    width: image.hidden ? 320 : image.naturalWidth,
    height: image.hidden ? 180 : image.naturalHeight,
  });
}

document.addEventListener("keydown", event => {
  if (event.ctrlKey && event.key.toLowerCase() === "a" && !event.altKey && !event.metaKey && !event.shiftKey) {
    event.preventDefault();
    return;
  }
  if (event.key === "Escape" || event.key === " ") {
    event.preventDefault();
    void close();
  }
});
document.querySelector<HTMLElement>("#close")!.addEventListener("click", () => { void close(); });
document.querySelector<HTMLElement>("#app")!.addEventListener("pointerdown", event => {
  if (event.button === 0 && !(event.target as HTMLElement).closest("#close, .page-indicators")) void largeWindow.startDragging();
});
document.addEventListener("wheel", event => {
  if (!carousel || !event.deltaY) return;
  event.preventDefault();
  const now = performance.now();
  if (now - lastWheelAt < 180) return;
  lastWheelAt = now;
  carousel.step(Math.sign(event.deltaY));
}, { passive: false });

async function start() {
  await listen<Payload>("large-preview-content", event => { void show(event.payload); });
  await listen("large-preview-clear", clear);
  await listen("large-preview-ping", () => { void emitTo("main", "large-preview-ready"); });
  await emitTo("main", "large-preview-ready");
}

void start();
