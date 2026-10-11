import type { PanelController } from "./panel";
import type { CommandCall } from "../services/desktop";
import { Observable } from "./observable";

export class RecordRenameController extends Observable<{ id: string | null; title: string; saving: boolean; focusRevision: number }> {
  private revision = 0;
  constructor(private panel: PanelController, private call: CommandCall, private toast: (message: string) => void,
    private focusRecord: (id: string) => void) {
    super({ id: null, title: "", saving: false, focusRevision: 0 });
  }
  start(id: string) {
    if (this.value.saving) return;
    const record = this.panel.value.snapshot?.records.find(record => record.id === id);
    if (!record) return;
    this.revision++;
    this.panel.selectRecord(id);
    this.update({ id, title: record.title ?? "", saving: false, focusRevision: this.value.focusRevision + 1 });
  }
  cancel(restoreFocus = true) {
    if (this.value.saving) return;
    const id = this.value.id;
    this.reset();
    if (id && restoreFocus) this.focusRecord(id);
  }
  reset() { this.revision++; this.update({ id: null, title: "", saving: false }); }
  reconcile() {
    if (this.value.id && !this.panel.records.some(record => record.id === this.value.id)) this.reset();
  }
  async commit() {
    const { id, saving } = this.value;
    if (!id || saving) return;
    const title = this.value.title.trim();
    if (Array.from(title).length > 100 || /[\r\n]/.test(title)) {
      this.toast("记录标题最多 100 个字符，且不能换行");
      this.update({ focusRevision: this.value.focusRevision + 1 }); return;
    }
    if (title === (this.panel.value.snapshot?.records.find(record => record.id === id)?.title ?? "")) { this.cancel(); return; }
    const revision = this.revision;
    this.update({ saving: true });
    try {
      await this.call("rename_record", { id, title });
      if (revision !== this.revision) return;
      this.reset();
      await this.panel.refresh();
      if (this.panel.value.active) this.focusRecord(id);
    } catch (error) {
      this.toast(String(error));
      if (revision === this.revision) this.update({ saving: false, focusRevision: this.value.focusRevision + 1 });
    }
  }
}
