import type { PanelSession, RecordItem, Settings, Snapshot } from "../domain/types";
import { filteredRecords, selectRange } from "../domain/records";
import type { CommandCall } from "../services/desktop";
import { Observable } from "./observable";

type PanelValue = PanelSession & {
  active: boolean; snapshot: Snapshot | null; settings: Settings | null;
  selectedId: string | null; selectedIds: ReadonlySet<string>; anchorId: string | null;
};
type PanelEffects = {
  reset: () => void; beforeRefresh: () => void; afterRefresh: () => void; afterBrowse: () => void;
  place: () => Promise<void>; afterOpen: () => void; isVisible: () => Promise<boolean>; error: (message: string) => void;
};

/** Owns the visible snapshot/session. Hidden panels never fetch or retain records. */
export class PanelController extends Observable<PanelValue> {
  generation = 0;
  private opening: Promise<void> | null = null;
  private refreshing: Promise<void> | null = null;
  private refreshAgain = false;
  constructor(private call: CommandCall, private effects: PanelEffects) {
    super({ active: false, snapshot: null, settings: null, view: "default", kindFilter: "all", search: "",
      selectedId: null, selectedIds: new Set(), anchorId: null });
  }
  get records(): RecordItem[] {
    return filteredRecords(this.value.snapshot?.records ?? [], this.value);
  }
  get selected(): RecordItem[] { return this.records.filter(record => this.value.selectedIds.has(record.id)); }
  saveSession() {
    const { view, kindFilter, search } = this.value;
    void this.call("save_panel_session", { session: { view, kindFilter, search } }).catch(() => {});
  }
  restore(session: PanelSession) { this.update(session); }
  browse(patch: Partial<PanelSession>) {
    this.effects.beforeRefresh();
    this.update(patch);
    this.reconcileSelection();
    this.effects.afterBrowse();
    this.saveSession();
  }
  selectGroup(view: string) {
    this.update({ selectedId: null, selectedIds: new Set(), anchorId: null });
    this.browse({ view });
  }
  cycleGroup(direction: number) {
    const views = ["default", "favorite", ...(this.value.snapshot?.groups ?? []).map(group => `group:${group.id}`)];
    this.selectGroup(views[(views.indexOf(this.value.view) + direction + views.length) % views.length]);
  }
  selectRecord(id: string, extend = false) {
    const selection = selectRange(this.records, this.value.anchorId, id, extend);
    if (selection) this.update(selection);
  }
  moveSelection(direction: number, extend = false) {
    const records = this.records;
    if (!records.length) return;
    const index = records.findIndex(record => record.id === this.value.selectedId);
    const next = index < 0 ? 0 : extend ? Math.max(0, Math.min(records.length - 1, index + direction))
      : (index + direction + records.length) % records.length;
    this.selectRecord(records[next].id, extend);
  }
  private reconcileSelection() {
    const records = this.records;
    const ids = new Set(records.map(record => record.id));
    let selectedIds = new Set([...this.value.selectedIds].filter(id => ids.has(id)));
    let { selectedId, anchorId } = this.value;
    if (!selectedId || !ids.has(selectedId)) selectedId = records.find(record => selectedIds.has(record.id))?.id ?? records[0]?.id ?? null;
    if (selectedId && !selectedIds.has(selectedId)) selectedIds = new Set([selectedId]);
    if (!anchorId || !ids.has(anchorId)) anchorId = selectedId;
    this.update({ selectedId, selectedIds, anchorId });
  }
  commitSettings(settings: Settings) {
    this.update({ settings, snapshot: this.value.snapshot ? { ...this.value.snapshot, settings } : null });
    if (!this.value.active) { this.clearHiddenSearch(); this.saveSession(); }
  }
  clearHiddenSearch() {
    if (!this.value.active && this.value.settings?.retainSearchInput === false) this.update({ search: "" });
  }
  refresh(): Promise<void> {
    if (!this.value.active) return Promise.resolve();
    this.refreshAgain = true;
    if (this.refreshing) return this.refreshing;
    this.refreshing = (async () => {
      while (this.refreshAgain && this.value.active) {
        this.refreshAgain = false;
        const generation = this.generation;
        const snapshot = await this.call<Snapshot>("get_state");
        if (!this.value.active || generation !== this.generation) continue;
        this.effects.beforeRefresh();
        this.update({ snapshot, settings: snapshot.settings });
        if (this.value.view.startsWith("group:") && !snapshot.groups.some(group => `group:${group.id}` === this.value.view)) {
          this.update({ view: "default" }); this.saveSession();
        }
        this.reconcileSelection();
        this.effects.afterRefresh();
      }
    })().finally(() => { this.refreshing = null; });
    return this.refreshing;
  }
  release() {
    if (!this.value.active) return;
    this.generation++;
    this.refreshAgain = false;
    this.update({ active: false, snapshot: null, selectedId: null, selectedIds: new Set(), anchorId: null });
    this.clearHiddenSearch();
    this.saveSession();
    this.effects.reset();
  }
  open(): Promise<void> {
    if (this.opening) return this.value.active ? this.opening : this.opening.then(() => this.open());
    this.effects.reset();
    this.update({ active: true, selectedId: null, selectedIds: new Set(), anchorId: null });
    const generation = ++this.generation;
    this.opening = (async () => {
      await this.refresh();
      if (!this.value.active || generation !== this.generation) return;
      await this.effects.place();
      if (!this.value.active || generation !== this.generation) return;
      await this.call("show_panel");
      if (!this.value.active || generation !== this.generation) return;
      this.effects.afterOpen();
    })().finally(() => { this.opening = null; });
    return this.opening;
  }
  async action(name: string, args: Record<string, unknown> = {}) {
    try {
      await this.call(name, args);
      if (name === "hide_panel") this.release();
      await this.refresh();
    } catch (error) { this.effects.error(String(error)); }
  }
  async apply(id: string) {
    try {
      await this.call("apply_record", { id });
      if (!await this.effects.isVisible()) this.release();
      await this.refresh();
    } catch (error) { this.effects.error(String(error)); }
  }
}
