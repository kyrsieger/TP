import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import type { Settings } from "../domain/types";
import { isMacOS } from "../services/desktop";
import type { SettingsController } from "../state/settings";
import { displayShortcut } from "../state/settings";
import type { PanelController } from "../state/panel";
import type { MenuController } from "../state/menus";
import { Icon } from "./Icon";
import { useObservable } from "./use-observable";
import { ThemeSetting } from "./ThemeSetting";
import { FontSetting } from "./FontSetting";
import { regularFontStyle } from "../domain/fonts";

function NumberSetting({ id, label, value, max, revision, save, toast }: { id: string; label: string; value: number; max: number; revision: number; save: (value: number) => void; toast: (message: string) => void }) {
  const [draft, setDraft] = useState(String(value));
  const timer = useRef<number | undefined>(undefined);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { setDraft(String(value)); }, [value, revision]);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const submit = (restoreInvalid: boolean) => {
    window.clearTimeout(timer.current);
    const element = input.current!, number = Number(element.value);
    if (!element.value || !element.validity.valid || !Number.isInteger(number)) {
      if (restoreInvalid) { setDraft(String(value)); toast("请输入有效的设置值"); }
      return;
    }
    save(number);
  };
  return <input ref={input} id={id} aria-label={label} type="number" min={1} max={max} value={draft}
    onChange={event => {
      setDraft(event.target.value);
      if (id === "sensitive") { window.clearTimeout(timer.current); timer.current = window.setTimeout(() => submit(false), 300); }
    }} onBlur={() => submit(true)} onKeyDown={event => {
      if (id === "max-ordinary" && event.key === "Enter") {
        event.preventDefault(); event.stopPropagation(); if (!event.repeat && !event.nativeEvent.isComposing) submit(true);
      }
    }} />;
}
const retentionOptions = [[1, "24 小时"], [3, "3 天"], [5, "5 天"], [7, "7 天"], [30, "1 个月"]] as const;
const positionOptions = [["left", "左下角"], ["right", "右下角"], ["center", "下方居中"]] as const;

export function SettingsPanel({ controller, history, menus, toast }: { controller: SettingsController; history: PanelController; menus: MenuController; toast: (message: string) => void }) {
  const state = useObservable(controller), panel = useObservable(history), menu = useObservable(menus);
  const [draft, setDraft] = useState<Settings | null>(panel.settings);
  const [version, setVersion] = useState("读取中…");
  useEffect(() => { setDraft(panel.settings); }, [panel.settings, state.revision]);
  useEffect(() => { let alive = true; void getVersion().then(value => { if (alive) setVersion(value); }).catch(() => { if (alive) setVersion("无法获取"); }); return () => { alive = false; }; }, []);
  const s = draft;
  const ordinaryCount = panel.snapshot?.records.filter(record => !record.favorite && record.groupId === null).length ?? 0;
  function save(patch: Partial<Settings>, reposition = false) {
    setDraft(previous => previous ? { ...previous, ...patch } : previous);
    void controller.save(patch, reposition);
  }
  if (!s) return null;
  return <div className="settings-layout">
    <div className="settings-content">
    <section className="settings-group" id="settings-appearance" aria-label="外观">
      <div className="settings-card">
        <ThemeSetting value={state.themeDraft ?? s.theme} open={menu.theme} toggle={theme => menus.update({ theme })} choose={theme => controller.selectTheme(theme)} />
        <FontSetting value={state.fontDraft?.family ?? s.fontFamily ?? ""} styleValue={state.fontDraft?.style ?? s.fontStyle ?? regularFontStyle}
          open={menu.font} toggle={font => menus.update({ font })} styleOpen={menu.fontStyle} toggleStyle={fontStyle => menus.update({ fontStyle })}
          choose={(family, style) => controller.selectFont(family, style)} />
      </div>
    </section>
    <section className="settings-group" id="settings-general" aria-label="通用">
      <div className="settings-card">
    <div className="settings-row"><span className="settings-copy">开机启动</span><input id="autostart" type="checkbox" aria-label="开机启动" checked={!!state.autostartEnabled} disabled={state.autostartEnabled === null || state.autostartSaving}
      onChange={event => { void controller.setAutostart(event.target.checked); }} /></div>
    <div className="settings-row"><span className="settings-copy">后台常驻<small id="background-resident-hint">开启后，关闭面板仍保留内容且不挂起，快捷键唤出更快，但会占用更多内存。</small></span><input id="background-resident" type="checkbox" aria-label="后台常驻" aria-describedby="background-resident-hint" checked={!!s.backgroundResident}
      onChange={event => save({ backgroundResident: event.target.checked })} /></div>
    {!isMacOS && <div className="settings-row"><span className="settings-copy">在任务栏显示图标<small id="show-taskbar-icon-hint">关闭后仍可通过系统托盘或唤出快捷键打开 TP。</small></span><input id="show-taskbar-icon" type="checkbox" aria-label="在任务栏显示图标" aria-describedby="show-taskbar-icon-hint" checked={s.showTaskbarIcon}
      onChange={event => save({ showTaskbarIcon: event.target.checked })} /></div>}
      </div>
    </section>
    <section className="settings-group" id="settings-panel" aria-label="面板行为">
      <div className="settings-card">
    <div className="settings-row"><span className="settings-copy">保留搜索框输入<small id="retain-search-input-hint">关闭后，每次面板关闭都会清空搜索框内的关键词。</small></span><input id="retain-search-input" type="checkbox" aria-label="保留搜索框输入" aria-describedby="retain-search-input-hint" checked={s.retainSearchInput} onChange={event => save({ retainSearchInput: event.target.checked })} /></div>
    <div className="settings-row"><span className="settings-copy">保留类型筛选<small id="retain-type-filter-hint">关闭后，每次面板关闭都会将类型筛选恢复为“全部”。</small></span><input id="retain-type-filter" type="checkbox" aria-label="保留类型筛选" aria-describedby="retain-type-filter-hint" checked={s.retainTypeFilter} onChange={event => save({ retainTypeFilter: event.target.checked })} /></div>
    <div className="settings-row"><span className="settings-copy">唤出快捷键</span><span className="settings-shortcut-control"><input id="shortcut" aria-label="唤出快捷键" placeholder="点击后按组合键" value={displayShortcut(state.shortcutDraft ?? s.shortcut)} readOnly onKeyDown={event => { event.preventDefault(); controller.captureShortcut(event.nativeEvent); }} />
      {state.shortcutError && <span className="shortcut-error" role="alert">{state.shortcutError}</span>}</span></div>
    <div className="settings-row"><span className="settings-copy">记录重命名快捷键</span><span className="settings-shortcut-control"><input id="rename-shortcut" aria-label="记录重命名快捷键" placeholder="点击后按快捷键" value={displayShortcut(state.renameShortcutDraft ?? s.renameShortcut)} readOnly
      onKeyDown={event => { event.preventDefault(); event.stopPropagation(); controller.captureRenameShortcut(event.nativeEvent); }} />
      {state.renameShortcutError && <span className="shortcut-error" role="alert">{state.renameShortcutError}</span>}</span></div>
    <p className="settings-card-note">默认为 F2，仅在记录列表中生效。可使用功能键或带修饰键的字母、数字；按 Backspace 或 Delete 清除。</p>
      </div>
    </section>
    <section className="settings-group" id="settings-records" aria-label="记录">
      <div className="settings-card">
    <div className="settings-row"><span className="settings-copy">普通记录数量上限<span className="setting-count">{ordinaryCount}/{panel.settings!.maxOrdinary}</span></span><NumberSetting id="max-ordinary" label="普通记录数量上限" value={s.maxOrdinary} max={500} revision={state.revision} save={maxOrdinary => save({ maxOrdinary })} toast={toast} /></div>
    <div className="setting-field"><span className="setting-label">普通记录保留时间</span><div className="setting-select retention-select">
      <button id="retention-trigger" type="button" aria-haspopup="listbox" aria-expanded={menu.retention} onClick={() => menus.update({ retention: !menu.retention })}><span className="retention-value">{retentionOptions.find(([value]) => value === s.retentionDays)?.[1] ?? `${s.retentionDays} 天`}</span><Icon name="chevron" /></button>
      <div id="retention-menu" role="listbox" hidden={!menu.retention}>{retentionOptions.map(([value, label]) => <button key={value} type="button" role="option" data-retention={value} aria-selected={s.retentionDays === value}
        onClick={() => { menus.update({ retention: false }); save({ retentionDays: value }); }}>{label}</button>)}</div>
    </div></div>
    <div className="settings-row"><span className="settings-copy">1Password 记录保留秒数</span><NumberSetting id="sensitive" label="1Password 记录保留秒数" value={s.sensitiveSeconds} max={4294967295} revision={state.revision} save={sensitiveSeconds => save({ sensitiveSeconds })} toast={toast} /></div>
      </div><p className="settings-group-note">数量和时间上限只作用于普通记录。收藏或已分组的记录只能手动删除。</p>
    </section>
    <section className="settings-group" id="settings-position" aria-label="面板位置">
    <fieldset className="settings-card" aria-label="面板位置">{[["fixed", "固定位置"], ["cursor", "跟随光标"]].map(([value, label]) => <div key={value} className="settings-row settings-radio"><input id={`position-mode-${value}`} type="radio" name="mode" value={value} aria-label={label} checked={s.positionMode === value}
      onChange={() => { if (value === "cursor") menus.update({ fixedPosition: false }); save({ positionMode: value as Settings["positionMode"] }); }} /><span className="settings-copy"><label htmlFor={`position-mode-${value}`}>{label}</label></span></div>)}
      <fieldset className="fixed-position-settings" aria-label="固定位置设置" disabled={s.positionMode !== "fixed"}>
        <div className="setting-field"><span className="setting-label">固定位置</span><div className="setting-select fixed-position-select">
          <button id="fixed-position-trigger" type="button" aria-haspopup="listbox" aria-expanded={menu.fixedPosition && s.positionMode === "fixed"} onClick={() => menus.update({ fixedPosition: !menu.fixedPosition })}><span className="fixed-position-value">{positionOptions.find(([value]) => value === s.fixedPosition)![1]}</span><Icon name="chevron" /></button>
          <div id="fixed-position-menu" role="listbox" hidden={!menu.fixedPosition || s.positionMode !== "fixed"}>{positionOptions.map(([value, label]) => <button key={value} type="button" role="option" data-fixed-position={value} aria-selected={s.fixedPosition === value}
            onClick={() => { menus.update({ fixedPosition: false }); save({ fixedPosition: value }, true); }}>{label}</button>)}</div>
        </div></div>
        <div className="settings-row"><span className="settings-copy">记忆拖拽位置</span><input id="remember" type="checkbox" aria-label="记忆拖拽位置" checked={s.rememberPosition} onChange={event => save({ rememberPosition: event.target.checked })} /></div>
      </fieldset>
    </fieldset>
    <p className="settings-group-note">拖拽面板边缘调整大小；缩放光标出现时，双击左右边缘恢复默认宽度，双击上下边缘恢复默认高度，双击角落恢复两维。大小会在下次打开时恢复。</p>
    </section>
    <section className="settings-group" id="settings-storage" aria-label="存储">
    <section className="settings-card cache-cleanup" aria-labelledby="cache-cleanup-title"><div className="cache-cleanup-copy"><h4 id="cache-cleanup-title">清理缓存</h4>
      <p className="hint">删除所有普通记录、对应的本地文件及孤立附件。收藏和自建分组中的记录会保留。当前可清理缓存大小：<span id="cache-size">{state.cacheSizeText}</span>。此操作无法撤销。</p></div>
      <button id="clear-cache" className="primary" disabled={state.clearingCache} onClick={() => { void controller.clearCache(); }}>{state.clearingCache ? "清理中…" : "清理缓存"}</button></section>
    <p className="settings-card app-version"><span>当前版本</span><span id="app-version">{version}</span></p>
    </section>
    <section className="settings-group" id="settings-shortcuts" aria-label="快捷键">
      <div className="settings-card"><ShortcutTable shortcut={s.shortcut} renameShortcut={s.renameShortcut} /></div>
    </section>
    </div>
  </div>;
}
function ShortcutTable({ shortcut, renameShortcut }: { shortcut: string; renameShortcut: string }) {
  const rows = [
    [shortcut ? displayShortcut(shortcut) : "未设置", "唤出面板（全局快捷键）"],
    [renameShortcut ? displayShortcut(renameShortcut) : "未设置", "重命名当前记录（仅记录列表）"],
    ["Ctrl+F", "聚焦搜索框"],
    [isMacOS ? "Delete / Fn+Delete" : "Backspace / Delete", "搜索框聚焦时编辑搜索词"],
    ["Tab / Shift+Tab", "切换到下一个 / 上一个分组"], ["←", "选中图片或视频记录时打开大图预览"], ["→", "显示当前选中记录的悬浮预览"],
    ["↑ / ↓", "选择上一条 / 下一条记录，首尾循环；分组菜单中切换选项"],
    ["Shift+↑ / Shift+↓、Shift+点击", "按当前列表顺序连续选择多条记录"],
    ["Space", "打开或关闭大图预览；分组菜单中确认选择"], ["1～9", "搜索框未聚焦时应用当前可见序号对应的记录"],
    ["Ctrl+1～9", "应用当前可见序号对应的记录，搜索框聚焦时也可使用"],
    ["Ctrl+D", "收藏或取消收藏所有选中记录"], [isMacOS ? "Ctrl+Delete" : "Ctrl+Backspace / Delete", "搜索框未聚焦时删除所有选中记录"],
    ["Ctrl+G", "打开分组菜单，移动所有选中记录"], ["Ctrl+P", "置顶或取消置顶所有选中记录"],
    ["Ctrl+A", "输入框聚焦时全选框内文字；面板其他区域禁用全选"],
  ];
  return <table className="shortcut-table"><thead><tr><th scope="col">按键</th><th scope="col">作用</th></tr></thead><tbody>{rows.map(([key, label]) => <tr key={label}><td><kbd>{key}</kbd></td><td>{label}</td></tr>)}</tbody></table>;
}
