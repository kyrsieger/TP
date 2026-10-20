import { PhysicalPosition, PhysicalSize, type Window } from "@tauri-apps/api/window";

import type { PanelController } from "../state/panel";
import { call, isMacOS } from "./desktop";
export class GeometryController {
  private placing = false;
  private positionSaveTimer?: number;
  private lastResizePress: {
    edge: string;
    time: number;
    x: number;
    y: number;
  } | null = null;
  constructor(private history: PanelController, private panel: Window, private toast: (message: string) => void) { }
  async placePanel() {
    const s = this.history.value.settings!;
    this.placing = true;
    try { await call("place_panel", { value: s }); }
    finally { window.setTimeout(() => this.placing = false, 350); }
  }
  scheduleGeometrySave() {
    if (this.placing || !this.history.value.settings || !this.history.value.active)
      return;
    window.clearTimeout(this.positionSaveTimer);
    this.positionSaveTimer = window.setTimeout(async () => {
      const position = await this.panel.outerPosition();
      const size = await this.panel.innerSize();
      const geometry = { panelX: position.x, panelY: position.y,
        panelWidth: size.width, panelHeight: size.height };
      try {
        await call("save_panel_geometry", { value: geometry });
        // Merge only coordinates into the latest settings. A delayed resize save
        // must never overwrite a switch changed while the request was in flight.
        const current = this.history.value.settings;
        if (current) this.history.commitSettings({ ...current, ...geometry });
      }
      catch { /* keep last valid size */ }
    }, 300);
  }
  async resize(event: PointerEvent, handle: HTMLElement) {
    if (event.button !== 0)
      return;
    event.preventDefault();
    const edge = handle.dataset.resizeEdge!;
    const now = performance.now();
    const previous = this.lastResizePress;
    this.lastResizePress = { edge, time: now, x: event.screenX, y: event.screenY };
    if (previous?.edge === edge && now - previous.time < 500 &&
      Math.abs(event.screenX - previous.x) < 6 && Math.abs(event.screenY - previous.y) < 6) {
      this.lastResizePress = null;
      try {
        const [size, scale] = await Promise.all([this.panel.innerSize(), this.panel.scaleFactor()]);
        await this.panel.setSize(new PhysicalSize(edge === "left" || edge === "right" ? Math.round(420 * scale) : size.width, edge === "top" || edge === "bottom" ? Math.round(600 * scale) : size.height));
        this.scheduleGeometrySave();
      }
      catch (error) {
        this.toast(String(error));
      }
      return;
    }
    if (isMacOS) {
      const start = { x: event.screenX, y: event.screenY };
      let latest = start;
      let initial: {
        size: PhysicalSize;
        position: PhysicalPosition;
        scale: number;
      } | null = null;
      let applying = false;
      let ended = false;
      const resize = async () => {
        if (!initial || applying || (latest.x === start.x && latest.y === start.y))
          return;
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
            await this.panel.setSize(new PhysicalSize(nextWidth, nextHeight));
            if (edge === "left" || edge === "top") {
              await this.panel.setPosition(new PhysicalPosition(initial.position.x + (edge === "left" ? initial.size.width - nextWidth : 0), initial.position.y + (edge === "top" ? initial.size.height - nextHeight : 0)));
            }
          } while (at !== latest);
        }
        finally {
          applying = false;
          if (ended)
            this.scheduleGeometrySave();
        }
      };
      const move = (next: PointerEvent) => {
        latest = { x: next.screenX, y: next.screenY };
        void resize().catch(error => this.toast(String(error)));
      };
      const end = (next: PointerEvent) => {
        move(next);
        ended = true;
        handle.removeEventListener("pointermove", move);
        handle.removeEventListener("pointerup", end);
        handle.removeEventListener("pointercancel", end);
        if (!applying && initial)
          this.scheduleGeometrySave();
      };
      handle.setPointerCapture(event.pointerId);
      handle.addEventListener("pointermove", move);
      handle.addEventListener("pointerup", end);
      handle.addEventListener("pointercancel", end);
      try {
        const [size, position, scale] = await Promise.all([this.panel.innerSize(), this.panel.outerPosition(), this.panel.scaleFactor()]);
        initial = { size, position, scale };
        await resize();
      }
      catch (error) {
        this.toast(String(error));
      }
      return;
    }
    try {
      await this.panel.startResizeDragging(({ left: "West", right: "East", top: "North", bottom: "South" } as const)[edge as "left" | "right" | "top" | "bottom"]);
    }
    catch (error) {
      this.toast(String(error));
    }
  }
  dispose() { window.clearTimeout(this.positionSaveTimer); }
}
