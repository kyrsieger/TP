import type { PanelController } from "./panel";
import type { CommandCall } from "../services/desktop";
import { Observable } from "./observable";

type GroupEdit = { mode: "create" | "rename"; id: number | null };
export class GroupController extends Observable<{ edit: GroupEdit | null; saving: boolean; name: string; focusRevision: number }> {
  constructor(private panel: PanelController, private call: CommandCall, private toast: (message: string) => void) {
    super({ edit: null, saving: false, name: "", focusRevision: 0 });
  }
  start(mode: "create" | "rename", id: number | null = null) {
    if (this.value.edit) return;
    const name = mode === "rename" ? this.panel.value.snapshot?.groups.find(group => group.id === id)?.name ?? "" : "";
    this.update({ edit: { mode, id }, name, focusRevision: this.value.focusRevision + 1 });
  }
  cancel() { if (!this.value.saving) this.update({ edit: null }); }
  reset() { this.update({ edit: null }); }
  async commit() {
    const { edit, saving } = this.value;
    if (!edit || saving) return;
    const name = this.value.name.trim();
    const groups = this.panel.value.snapshot?.groups ?? [];
    if (edit.mode === "rename" && name === groups.find(group => group.id === edit.id)?.name) { this.cancel(); return; }
    if (!name || name === "默认" || name === "收藏" || groups.some(group => group.name === name && group.id !== edit.id)) {
      this.toast("分组名称不可用"); this.update({ focusRevision: this.value.focusRevision + 1 }); return;
    }
    this.update({ saving: true });
    try {
      await this.call(edit.mode === "create" ? "add_group" : "rename_group", edit.mode === "create" ? { name } : { id: edit.id, name });
      this.update({ edit: null }); await this.panel.refresh();
    } catch (error) { this.toast(String(error)); this.update({ focusRevision: this.value.focusRevision + 1 }); }
    finally { this.update({ saving: false }); }
  }
}
