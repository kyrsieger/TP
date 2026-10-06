import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";

type Label = "preview" | "large-preview";
type WindowState = { opening: Promise<void> | null; releasing: Promise<void> | null; timer?: number };
const windows: Record<Label, WindowState> = {
  preview: { opening: null, releasing: null },
  "large-preview": { opening: null, releasing: null },
};

function cancelRelease(label: Label) {
  window.clearTimeout(windows[label].timer);
  windows[label].timer = undefined;
}

async function waitForWindowEvent(label: Label, event: string, action: () => Promise<boolean>) {
  let resolveReady!: () => void;
  let rejectReady!: (error: Error) => void;
  const ready = new Promise<void>((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
  const unlisten = await listen(event, resolveReady);
  const timer = window.setTimeout(() => rejectReady(new Error(`${label} 窗口响应超时`)), 10000);
  try {
    await Promise.all([ready, action().then(wait => { if (!wait) resolveReady(); })]);
  } finally {
    window.clearTimeout(timer);
    unlisten();
  }
}

export function ensurePreviewWindow(label: Label): Promise<void> {
  const state = windows[label];
  cancelRelease(label);
  if (!state.opening) {
    state.opening = (async () => {
      await state.releasing;
      await waitForWindowEvent(label, `${label}-ready`, async () => {
        await invoke("ensure_preview_window", { label });
        await emitTo(label, `${label}-ping`);
        return true;
      });
    })().finally(() => { state.opening = null; });
  }
  return state.opening;
}

export async function initPreviewWindows() {
  const used = await listen<Label>("preview-window-used", event => { cancelRelease(event.payload); });
  const hidden = await listen<Label>("preview-window-hidden", event => {
    const label = event.payload;
    const state = windows[label];
    if (state.timer !== undefined || state.releasing) return;
    state.timer = window.setTimeout(() => {
      state.timer = undefined;
      const opening = state.opening;
      state.releasing = (async () => {
        await opening;
        await waitForWindowEvent(label, `${label}-destroyed`, () => invoke<boolean>("release_preview_window", { label }));
      })().catch(error => { console.error("释放预览窗口失败", error); })
        .finally(() => { state.releasing = null; });
    }, 30000);
  });
  return () => { used(); hidden(); cancelRelease("preview"); cancelRelease("large-preview"); };
}
