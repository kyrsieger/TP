import { t } from "../i18n";
import { emitTo, listen } from "@tauri-apps/api/event";
import { monitorFromPoint, type Window } from "@tauri-apps/api/window";
import { call } from "./desktop";
import { subscribeAll } from "./subscriptions";
import { ensurePreviewWindow, initPreviewWindows } from "../preview-windows";
import { previewPaths } from "../domain/records";
import type { PanelController } from "../state/panel";
import type { RecordViewport } from "./viewport";
export class PreviewController {
  hoveredId: string | null = null;
  previewFromKeyboard = false;
  private previewId: string | null = null;
  private previewHovered = false;
  largePreviewId: string | null = null;
  private largePreviewSizeRequest = 0;
  private hoverTimer?: number;
  private previewCloseTimer?: number;
  private previewGeneration = 0;
  private previewPlacement: {
    x: number;
    rowY: number;
    width: number;
    scale: number;
    top: number;
    bottom: number;
  } | null = null;
  constructor(private history: PanelController, private panel: Window, private list: RecordViewport, private toast: (message: string) => void) { }
  reset() { this.closePreview(); this.largePreviewId = null; this.largePreviewSizeRequest++; }
  leave(id: string) {
    if (this.hoveredId !== id)
      return;
    this.hoveredId = null;
    window.clearTimeout(this.hoverTimer);
    this.schedulePreviewClose();
  }
  reconcile() {
    if (this.largePreviewId && !this.history.records.some(record => record.id === this.largePreviewId)) {
      this.largePreviewId = null;
      void call("hide_large_preview").catch(() => { });
    }
  }
  closePreview() {
    if (this.previewId)
      void emitTo("preview", "preview-clear", this.previewGeneration).catch(() => { });
    window.clearTimeout(this.hoverTimer);
    window.clearTimeout(this.previewCloseTimer);
    this.hoveredId = null;
    this.previewFromKeyboard = false;
    this.previewId = null;
    this.previewHovered = false;
    this.previewPlacement = null;
    this.previewGeneration++;
    void call("hide_preview").catch(() => { });
  }
  schedulePreviewClose() {
    window.clearTimeout(this.previewCloseTimer);
    this.previewCloseTimer = window.setTimeout(() => {
      if (!this.hoveredId && !this.previewHovered)
        this.closePreview();
    }, 220);
  }
  async preparePreview(id: string) {
    if (this.hoveredId !== id)
      return;
    const token = ++this.previewGeneration;
    const row = this.list.row(id);
    const record = this.history.value.snapshot?.records.find(item => item.id === id);
    if (!row || !record)
      return;
    try {
      await ensurePreviewWindow("preview");
      if (token !== this.previewGeneration || this.hoveredId !== id) {
        if (!this.hoveredId)
          void call("hide_preview").catch(() => { });
        return;
      }
      const [position, size, scale] = await Promise.all([this.panel.outerPosition(), this.panel.outerSize(), this.panel.scaleFactor()]);
      const monitor = await monitorFromPoint(position.x + size.width / 2, position.y + size.height / 2);
      if (!monitor || token !== this.previewGeneration || this.hoveredId !== id)
        return;
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
      this.previewId = id;
      this.previewPlacement = { x, rowY, width, scale, top: area.position.y, bottom: area.position.y + area.size.height };
      await emitTo("preview", "preview-content", {
        theme: this.history.value.settings?.theme,
        removeTrailingNewlines: this.history.value.settings?.removeTrailingNewlines !== false,
        id, kind: record.kind, body: record.body, title: record.title, files: previewPaths(record),
      groupName: this.history.value.snapshot?.groups.find(group => group.id === record.groupId)?.name ?? null,
        preview: record.preview, status: record.status,
        search: this.history.value.search,
        width: width / scale,
        maxHeight: (area.size.height - 2 * margin) / scale,
        generation: token,
      });
    }
    catch (error) {
      if (token !== this.previewGeneration)
        return;
      void call("hide_preview").catch(() => { });
      this.toast(t("预览失败：{0}", String(error)));
    }
  }
  hoverRecord(id: string) {
    if (this.largePreviewId)
      return;
    if (this.hoveredId === id)
      return;
    this.closePreview();
    this.hoveredId = id;
    this.hoverTimer = window.setTimeout(() => { void this.preparePreview(id); }, 300);
  }
  showHoverPreview(id: string) {
    if (this.largePreviewId || !this.history.value.snapshot || this.hoveredId === id)
      return;
    const row = this.list.ensure(id);
    if (!row)
      return;
    row.scrollIntoView({ block: "nearest" });
    window.requestAnimationFrame(() => {
      if (this.largePreviewId || this.hoveredId === id)
        return;
      if (!this.list.row(id) || !this.history.value.snapshot?.records.some(item => item.id === id))
        return;
      this.closePreview();
      this.hoveredId = id;
      this.previewFromKeyboard = true;
      window.clearTimeout(this.hoverTimer);
      this.hoverTimer = window.setTimeout(() => { void this.preparePreview(id); }, 0);
    });
  }
  async openLargePreview(id: string) {
    if (this.largePreviewId)
      return;
    const record = this.history.value.snapshot?.records.find(item => item.id === id);
    if (!record || (record.kind !== "image" && record.kind !== "video"))
      return;
    if (record.kind === "video" && !record.preview && record.files.length < 2) {
      this.toast(t("首帧预览不可用"));
      return;
    }
    this.closePreview();
    this.largePreviewId = id;
    const request = ++this.largePreviewSizeRequest;
    try {
      await call<boolean>("open_native_large_preview", { id, index: 0 });
      if (request !== this.largePreviewSizeRequest)
        return;
      if (this.largePreviewId !== id) await call("hide_large_preview");
    }
    catch (error) {
      if (this.largePreviewId !== id || request !== this.largePreviewSizeRequest)
        return;
      this.largePreviewId = null;
      void call("hide_large_preview").catch(() => { });
      this.toast(t("预览失败：{0}", String(error)));
    }
  }
  async start() {
    const disposeSubscriptions = await subscribeAll([
      initPreviewWindows(),
      listen("large-preview-closed", () => { this.largePreviewId = null; }),
      listen<string>("native-preview-error", event => { this.toast(t("预览失败：{0}", event.payload)); }),
      listen<{
        id: string;
        generation: number;
        height: number;
      }>("preview-measured", event => {
        const { id, generation, height: measured } = event.payload;
        const placement = this.previewPlacement;
        if (id !== this.previewId || generation !== this.previewGeneration || !placement || (!this.hoveredId && !this.previewHovered))
          return;
        const margin = Math.round(12 * placement.scale);
        const height = Math.max(1, Math.min(Math.ceil(measured * placement.scale), placement.bottom - placement.top - 2 * margin));
        const y = Math.max(placement.top + margin, Math.min(placement.rowY, placement.bottom - margin - height));
        const token = this.previewGeneration;
        void call("show_preview", { x: placement.x, y, width: placement.width, height }).then(() => {
          if (token !== this.previewGeneration)
            void call("hide_preview").catch(() => { });
        }).catch(error => this.toast(t("预览失败：{0}", String(error))));
      }),
      listen<string>("preview-enter", event => {
        if (event.payload !== this.previewId)
          return;
        this.previewHovered = true;
        window.clearTimeout(this.previewCloseTimer);
      }),
      listen<string>("preview-leave", event => {
        if (event.payload !== this.previewId)
          return;
        this.previewHovered = false;
        this.schedulePreviewClose();
      }),
      listen<string>("preview-blur", event => {
        if (event.payload === this.previewId)
          this.closePreview();
      }),
      listen<string>("preview-space", event => {
        if (event.payload === this.history.value.selectedId)
          void this.openLargePreview(event.payload);
      }),
      ]);
    return () => { disposeSubscriptions(); this.reset(); };
  }
}
