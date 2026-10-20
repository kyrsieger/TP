import type { Settings, SettingsSaveResult } from "../domain/types";
import type { CommandCall } from "../services/desktop";
import type { PanelController } from "./panel";
import { Observable } from "./observable";
import { recordRenameShortcutFromKey, recordRenameShortcutError } from "../domain/shortcuts";
import { getTheme, defaultTheme, type ThemeId } from "../domain/themes";
import { fontStyleKey, regularFontStyle, type FontSelection, type FontStyle } from "../domain/fonts";

export const displayShortcut = (shortcut: string) => shortcut.split("+").map(part => part === "Control" ? "Ctrl" : part).join("+");
export function shortcutFromKey(event: KeyboardEvent): string | null {
  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
  if (["Control", "Alt", "Shift", "Meta"].includes(key)) return null;
  const modifiers = [event.ctrlKey && "Control", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Super"].filter(Boolean);
  if (!modifiers.length || !/^([A-Z0-9]|F\d{1,2})$/.test(key)) return null;
  return [...modifiers, key].join("+");
}
export function formatCacheSize(bytes: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = bytes, unit = 0;
  while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit++; }
  return `${unit === 0 ? size : size.toFixed(1)} ${units[unit]}`;
}
type SettingsValue = {
  themeDraft: ThemeId | null;
  fontDraft: FontSelection | null;
  open: boolean; clearingCache: boolean; releasingPanel: boolean; cacheSizeText: string;
  shortcutDraft: string | null; shortcutError: string | null;
  renameShortcutDraft: string | null; renameShortcutError: string | null;
  autostartEnabled: boolean | null; autostartSaving: boolean;
  confirmationCount: number | null; cacheConfirmationOpen: boolean; revision: number;
};
type SettingsEffects = {
  toast: (message: string) => void; place: () => Promise<void>;
  position: () => Promise<{ x: number; y: number }>;
};

/** Serializes configuration writes and owns the explicit cleanup confirmation. */
export class SettingsController extends Observable<SettingsValue> {
  private activeShortcut = "";
  private cacheRequest = 0;
  private cacheClearQueued = false;
  private queue: Promise<void> = Promise.resolve();
  private pending: Settings | null = null;
  private resolveConfirmation: ((confirmed: boolean) => void) | null = null;
  private themeTimer: ReturnType<typeof setTimeout> | undefined;
  private themeGeneration = 0;
  private fontGeneration = 0;
  constructor(private history: PanelController, private call: CommandCall, private effects: SettingsEffects) {
    super({ themeDraft: null, fontDraft: null, open: false, clearingCache: false, releasingPanel: false, cacheSizeText: "计算中…", shortcutDraft: null,
      shortcutError: null, renameShortcutDraft: null, renameShortcutError: null, autostartEnabled: null, autostartSaving: false, confirmationCount: null, cacheConfirmationOpen: false, revision: 0 });
  }
  ready(error: string | null) {
    this.update({ shortcutError: error });
    if (error) this.effects.toast(error);
  }
  syncShortcut() { this.activeShortcut = this.value.shortcutError ? "" : this.history.value.settings?.shortcut ?? ""; }
  selectFont(fontFamily: string, fontStyle: FontStyle = regularFontStyle) {
    const previous = this.value.fontDraft ?? { family: this.history.value.settings?.fontFamily ?? "", style: this.history.value.settings?.fontStyle ?? regularFontStyle };
    if (fontFamily === previous.family && fontStyleKey(fontStyle) === fontStyleKey(previous.style)) return;
    const { name, family, weight, stretch, style } = fontStyle;
    const selection = { family: fontFamily, style: { name, family, weight, stretch, style } };
    const generation = ++this.fontGeneration;
    this.update({ fontDraft: selection });
    void this.save({ fontFamily, fontStyle: selection.style }).then(() => {
      if (generation === this.fontGeneration) this.update({ fontDraft: null });
    });
  }
  reset() { this.flushTheme(); this.confirm(false); this.update({ open: false }); this.cacheRequest++; }
  selectTheme(id: ThemeId) {
    const theme = getTheme(id).id;
    if (theme === (this.value.themeDraft ?? this.history.value.settings?.theme ?? defaultTheme)) return;
    this.themeGeneration++;
    this.update({ themeDraft: theme });
    clearTimeout(this.themeTimer);
    this.themeTimer = setTimeout(() => this.flushTheme(), 160);
  }
  flushTheme() {
    if (this.themeTimer === undefined) return;
    clearTimeout(this.themeTimer); this.themeTimer = undefined;
    const theme = this.value.themeDraft, generation = this.themeGeneration;
    if (theme) void this.save({ theme }).then(() => {
      // A slow earlier write must never overwrite the theme currently under the pointer.
      if (generation === this.themeGeneration) this.update({ themeDraft: null });
    });
  }
  async open() {
    this.update({ open: true, cacheSizeText: "计算中…" });
    void this.refreshCacheSize();
    if (this.value.autostartSaving) return;
    try { this.update({ autostartEnabled: await this.call<boolean>("autostart_enabled") }); }
    catch (error) { this.update({ autostartEnabled: null }); this.effects.toast(String(error)); }
  }
  async refreshCacheSize() {
    const request = ++this.cacheRequest;
    let cacheSizeText: string;
    try { cacheSizeText = formatCacheSize(await this.call<number>("get_cache_size")); }
    catch { cacheSizeText = "无法获取"; }
    if (request === this.cacheRequest && this.value.open) this.update({ cacheSizeText });
  }
  async setAutostart(enabled: boolean) {
    const previous = this.value.autostartEnabled;
    this.update({ autostartEnabled: enabled, autostartSaving: true });
    try { await this.call("set_autostart", { enabled }); }
    catch (error) { this.update({ autostartEnabled: previous }); this.effects.toast(String(error)); }
    finally { this.update({ autostartSaving: false }); }
  }
  async releasePanelNow() {
    if (this.value.releasingPanel || !this.value.open || !this.history.value.active) return;
    this.update({ releasingPanel: true });
    try {
      this.flushTheme();
      await this.queue;
      if (!this.value.open || !this.history.value.active) return;
      if (this.history.value.settings?.backgroundResident) {
        this.effects.toast("请先关闭后台常驻再测试");
        return;
      }
      await this.call("debug_release_panel");
    } catch (error) { this.effects.toast(`释放面板失败：${String(error)}`); }
    finally { this.update({ releasingPanel: false }); }
  }
  async clearCache() {
    if (this.cacheClearQueued || !this.value.open || !this.history.value.active) return;
    this.cacheClearQueued = true;
    const generation = this.history.generation;
    // Share the settings queue so a pending save and cache cleanup cannot replace each other's confirmation.
    this.queue = this.queue.then(async () => {
      if (!this.value.open || !this.history.value.active || generation !== this.history.generation) return;
      const confirmed = await new Promise<boolean>(resolve => {
        this.resolveConfirmation = resolve;
        this.update({ cacheConfirmationOpen: true });
      });
      if (!confirmed || !this.value.open || !this.history.value.active || generation !== this.history.generation) return;
      this.update({ clearingCache: true });
      try {
        const count = await this.call<number>("clear_cache");
        await this.history.refresh();
        this.effects.toast(`已清理 ${count} 条普通记录及孤立附件`);
      } catch (error) {
        await this.history.refresh().catch(() => {});
        this.effects.toast(`清理缓存失败：${String(error)}`);
      } finally { this.update({ clearingCache: false }); }
    });
    try { await this.queue; }
    finally { this.cacheClearQueued = false; }
  }
  confirm(confirmed: boolean) {
    const resolve = this.resolveConfirmation;
    this.resolveConfirmation = null;
    this.update({ confirmationCount: null, cacheConfirmationOpen: false });
    resolve?.(confirmed);
  }
  private confirmCleanup(count: number) {
    return new Promise<boolean>(resolve => {
      this.resolveConfirmation = resolve;
      this.update({ confirmationCount: count });
    });
  }
  private async registerShortcut(shortcut: string) {
    if (shortcut === this.activeShortcut) return;
    await this.call("register_panel_shortcut", { shortcut });
    this.activeShortcut = shortcut;
  }
  captureShortcut(event: KeyboardEvent) {
    if (event.repeat || event.isComposing || event.keyCode === 229) return;
    const shortcut = event.key === "Backspace" || event.key === "Delete" ? "" : shortcutFromKey(event);
    if (shortcut === null) return;
    if (shortcut && shortcut === (this.history.value.settings?.renameShortcut ?? "F2")) {
      this.update({ shortcutDraft: shortcut, shortcutError: "此快捷键已用于记录重命名，请更换组合键。" });
      return;
    }
    this.update({ shortcutDraft: shortcut, shortcutError: null });
    void this.save({ shortcut });
  }
  captureRenameShortcut(event: KeyboardEvent) {
    if (event.repeat || event.isComposing || event.keyCode === 229) return;
    const shortcut = event.key === "Backspace" || event.key === "Delete" ? "" : recordRenameShortcutFromKey(event);
    if (shortcut === null) return;
    const error = recordRenameShortcutError(shortcut, this.history.value.settings?.shortcut ?? "");
    this.update({ renameShortcutDraft: shortcut, renameShortcutError: error });
    if (!error) void this.save({ renameShortcut: shortcut });
  }
  save(patch: Partial<Settings>, reposition = false): Promise<void> {
    if (this.pending && (patch.maxOrdinary !== undefined || patch.retentionDays !== undefined)
      && Object.keys(patch).every(key => this.pending![key as keyof Settings] === patch[key as keyof Settings])) return this.queue;
    this.queue = this.queue.then(async () => {
      const previous = this.history.value.settings;
      if (!previous) return;
      const next = { ...previous, ...patch };
      if (Object.keys(patch).every(key => previous[key as keyof Settings] === next[key as keyof Settings])
        && (patch.shortcut === undefined || patch.shortcut === this.activeShortcut)) {
        if (patch.shortcut !== undefined && this.value.shortcutDraft === next.shortcut) this.update({ shortcutDraft: null });
        if (patch.renameShortcut !== undefined && this.value.renameShortcutDraft === next.renameShortcut) this.update({ renameShortcutDraft: null, renameShortcutError: null });
        return;
      }
      try {
        if (patch.rememberPosition && !previous.rememberPosition) {
          const position = await this.effects.position(); next.panelX = position.x; next.panelY = position.y;
        }
        if (patch.shortcut !== undefined) await this.registerShortcut(next.shortcut);
        this.pending = next;
        let confirmedCount: number | undefined;
        while (true) {
          const result = await this.call<SettingsSaveResult>("save_settings", { value: next, confirmedCount });
          if (result.status === "saved") break;
          if (!this.history.value.active || !await this.confirmCleanup(result.count)) return;
          confirmedCount = result.count;
        }
        this.history.commitSettings(next);
        if (patch.shortcut !== undefined) this.update({
          shortcutDraft: this.value.shortcutDraft === next.shortcut ? null : this.value.shortcutDraft, shortcutError: null,
        });
        if (patch.renameShortcut !== undefined) this.update({
          renameShortcutDraft: this.value.renameShortcutDraft === next.renameShortcut ? null : this.value.renameShortcutDraft,
          renameShortcutError: this.value.renameShortcutDraft === next.renameShortcut ? null : this.value.renameShortcutError,
        });
        if (reposition) await this.effects.place().catch(error => this.effects.toast(String(error)));
      } catch (error) {
        if (patch.shortcut !== undefined) await this.registerShortcut(previous.shortcut).catch(() => {});
        if (patch.renameShortcut !== undefined) {
          const message = `无法保存重命名快捷键：${String(error)}`;
          if (this.value.renameShortcutDraft === next.renameShortcut) this.update({ renameShortcutError: message });
          this.effects.toast(message);
          return;
        }
        let shortcutError = this.value.shortcutError;
        if (patch.shortcut !== undefined && this.value.shortcutDraft === next.shortcut) {
          const detail = String(error);
          shortcutError = !next.shortcut ? `无法清除快捷键：${detail}` : /already registered/i.test(detail)
            ? `${displayShortcut(next.shortcut)} 已被占用，请更换组合键或解除占用。`
            : `无法启用 ${displayShortcut(next.shortcut)}：${detail}`;
          this.update({ shortcutError });
        }
        this.effects.toast(shortcutError ?? String(error));
      } finally {
        this.pending = null;
        this.update({ revision: this.value.revision + 1 });
      }
    });
    return this.queue;
  }
}
