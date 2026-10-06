import type { RecordViewport } from "./viewport";

type FocusTarget = { kind: "id" | "record" | "view"; value: string };
export class SettingsFocus {
  private target: FocusTarget | null = null;
  capture() {
    const element = document.activeElement as HTMLElement | null;
    this.target = null;
    if (!element || element === document.body || element === document.documentElement || element.closest("#settings-button")) return;
    const record = element.closest<HTMLElement>(".record[data-id]"), view = element.closest<HTMLElement>("#groups [data-view]");
    if (record?.dataset.id) this.target = { kind: "record", value: record.dataset.id };
    else if (view?.dataset.view) this.target = { kind: "view", value: view.dataset.view };
    else if (element.id) this.target = { kind: "id", value: element.id };
  }
  reset() { this.target = null; }
  restore(list: RecordViewport) {
    const previous = this.target;
    this.target = null;
    const target = previous?.kind === "id" ? document.getElementById(previous.value)
      : previous?.kind === "record" ? list.ensure(previous.value)
      : previous ? document.querySelector<HTMLElement>(`#groups [data-view="${CSS.escape(previous.value)}"]`) : null;
    const fallback = target ?? (previous ? document.querySelector<HTMLInputElement>("#search") : null);
    if (fallback) { fallback.focus({ preventScroll: true }); if (fallback.classList.contains("record")) fallback.scrollIntoView({ block: "nearest" }); }
    else if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  }
}
