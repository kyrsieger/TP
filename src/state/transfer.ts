import { Observable } from "./observable";
import type { CommandCall } from "../services/desktop";
import type { PanelController } from "./panel";

export type RestorePreview = {
  token: string; records: number; groups: number; cleanupCount: number;
  currentMaxOrdinary: number; nextMaxOrdinary: number; warnings: string[];
};
type ExportResult = { path: string; records: number; excludedSensitive: number; warnings: string[] };
type RestoreResult = { status: "confirmation_required"; preview: RestorePreview }
  | { status: "completed"; records: number; cleaned: number };
type TransferValue = {
  mode: "export" | "import" | null; busy: boolean; preview: RestorePreview | null;
  keepExpired: boolean; error: string | null;
};

export class TransferController extends Observable<TransferValue> {
  private generation = 0;
  constructor(private history: PanelController, private call: CommandCall, private toast: (message: string) => void,
    private serialize: (task: () => Promise<void>) => Promise<void>) {
    super({ mode: null, busy: false, preview: null, keepExpired: false, error: null });
  }
  requestExport() {
    if (this.value.busy || this.value.mode) return;
    this.update({ mode: "export", error: null });
  }
  cancel() {
    if (this.value.busy) return;
    const token = this.value.preview?.token;
    this.generation++;
    this.update({ mode: null, preview: null, error: null });
    if (token) void this.call("discard_backup", { token }).catch(() => {});
  }
  reset() {
    if (!this.value.busy) this.cancel();
    else if (this.value.mode === "import" && !this.value.preview) {
      this.generation++;
      this.update({ mode: null });
    }
  }
  async inspect() {
    if (this.value.busy || this.value.mode) return;
    const generation = ++this.generation;
    this.update({ mode: "import", busy: true, error: null });
    try {
      await this.serialize(async () => {
        const preview = await this.call<RestorePreview | null>("inspect_backup");
        if (generation !== this.generation) {
          if (preview) await this.call("discard_backup", { token: preview.token });
          return;
        }
        this.update({ preview, mode: preview ? "import" : null, keepExpired: false });
      });
    } catch (error) { this.update({ mode: null }); this.toast(`无法读取导出包：${String(error)}`); }
    finally { this.update({ busy: false }); }
  }
  async confirm() {
    if (this.value.busy || !this.value.mode) return;
    const { mode, preview, keepExpired } = this.value;
    if (mode === "import" && !preview) return;
    this.update({ busy: true, error: null });
    try {
      await this.serialize(async () => {
        if (mode !== "import") {
          await this.call<ExportResult | null>("export_data");
          this.update({ mode: null });
          return;
        }
        const result = await this.call<RestoreResult>("restore_backup", {
          token: preview!.token, keepExpired, expectedCleanupCount: preview!.cleanupCount, expectedMaxOrdinary: preview!.nextMaxOrdinary,
        });
        if (result.status === "confirmation_required") {
          this.update({ preview: result.preview, error: "保留规则或过期数量已变化，请核对后再次确认。" });
          return;
        }
        this.update({ mode: null, preview: null });
        await this.history.refresh().catch(error => this.toast(`记录已导入，刷新界面失败：${String(error)}`));
      });
    } catch (error) {
      if (mode === "import") {
        // A failed replacement consumes its prepared session. Require inspection again.
        this.update({ mode: null, preview: null });
        this.toast(`导入失败：${String(error)}`);
      } else this.update({ error: `保存失败：${String(error)}` });
    } finally { this.update({ busy: false }); }
  }
}
