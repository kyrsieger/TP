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

function NumberSetting({ id, value, max, revision, save, toast }: { id: string; value: number; max: number; revision: number; save: (value: number) => void; toast: (message: string) => void }) {
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
  return <input ref={input} id={id} type="number" min={1} max={max} value={draft}
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
  return <>
    <h2>设置</h2><label className="inline"><input id="autostart" type="checkbox" checked={!!state.autostartEnabled} disabled={state.autostartEnabled === null || state.autostartSaving}
      onChange={event => { void controller.setAutostart(event.target.checked); }} />开机启动</label>
    {!isMacOS && <><label className="inline"><input id="show-taskbar-icon" type="checkbox" checked={s.showTaskbarIcon}
      onChange={event => save({ showTaskbarIcon: event.target.checked })} />在任务栏显示图标</label>
      <p className="hint">关闭后仍可通过系统托盘或唤出快捷键打开 TP。</p></>}
    <label className="inline"><input id="retain-search-input" type="checkbox" checked={s.retainSearchInput} onChange={event => save({ retainSearchInput: event.target.checked })} />保留搜索框输入</label>
    <p className="hint">关闭后，每次面板关闭都会清空搜索框内的关键词。</p>
    <label>唤出快捷键<input id="shortcut" placeholder="点击后按组合键" value={displayShortcut(state.shortcutDraft ?? s.shortcut)} readOnly onKeyDown={event => { event.preventDefault(); controller.captureShortcut(event.nativeEvent); }} />
      {state.shortcutError && <span className="shortcut-error" role="alert">{state.shortcutError}</span>}</label>
    <label>记录重命名快捷键<input id="rename-shortcut" placeholder="点击后按快捷键" value={displayShortcut(state.renameShortcutDraft ?? s.renameShortcut)} readOnly
      onKeyDown={event => { event.preventDefault(); event.stopPropagation(); controller.captureRenameShortcut(event.nativeEvent); }} />
      {state.renameShortcutError && <span className="shortcut-error" role="alert">{state.renameShortcutError}</span>}</label>
    <p className="hint">默认为 F2，仅在记录列表中生效。可使用功能键或带修饰键的字母、数字；按 Backspace 或 Delete 清除。</p>
    <label><span>普通记录数量上限<span className="setting-count">{ordinaryCount}/{panel.settings!.maxOrdinary}</span></span><NumberSetting id="max-ordinary" value={s.maxOrdinary} max={500} revision={state.revision} save={maxOrdinary => save({ maxOrdinary })} toast={toast} /></label>
    <div className="setting-field"><span className="setting-label">普通记录保留时间</span><div className="setting-select retention-select">
      <button id="retention-trigger" type="button" aria-haspopup="listbox" aria-expanded={menu.retention} onClick={() => menus.update({ retention: !menu.retention })}><span className="retention-value">{retentionOptions.find(([value]) => value === s.retentionDays)?.[1] ?? `${s.retentionDays} 天`}</span><Icon name="chevron" /></button>
      <div id="retention-menu" role="listbox" hidden={!menu.retention}>{retentionOptions.map(([value, label]) => <button key={value} type="button" role="option" data-retention={value} aria-selected={s.retentionDays === value}
        onClick={() => { menus.update({ retention: false }); save({ retentionDays: value }); }}>{label}</button>)}</div>
    </div></div>
    <label>1Password 记录保留秒数<NumberSetting id="sensitive" value={s.sensitiveSeconds} max={4294967295} revision={state.revision} save={sensitiveSeconds => save({ sensitiveSeconds })} toast={toast} /></label>
    <p className="hint">数量和时间上限只作用于普通记录。收藏或已分组的记录只能手动删除。</p>
    <fieldset><legend>面板位置</legend>{[["fixed", "固定位置"], ["cursor", "跟随光标"]].map(([value, label]) => <label key={value} className="inline"><input type="radio" name="mode" value={value} checked={s.positionMode === value}
      onChange={() => { if (value === "cursor") menus.update({ fixedPosition: false }); save({ positionMode: value as Settings["positionMode"] }, true); }} />{label}</label>)}</fieldset>
    <div className="setting-field"><span className="setting-label">固定位置</span><div className="setting-select fixed-position-select">
      <button id="fixed-position-trigger" type="button" aria-haspopup="listbox" aria-expanded={menu.fixedPosition} disabled={s.positionMode === "cursor"} onClick={() => menus.update({ fixedPosition: !menu.fixedPosition })}><span className="fixed-position-value">{positionOptions.find(([value]) => value === s.fixedPosition)![1]}</span><Icon name="chevron" /></button>
      <div id="fixed-position-menu" role="listbox" hidden={!menu.fixedPosition}>{positionOptions.map(([value, label]) => <button key={value} type="button" role="option" data-fixed-position={value} aria-selected={s.fixedPosition === value}
        onClick={() => { menus.update({ fixedPosition: false }); save({ fixedPosition: value }, true); }}>{label}</button>)}</div>
    </div></div>
    <label className="inline"><input id="remember" type="checkbox" checked={s.rememberPosition} disabled={s.positionMode === "cursor"} onChange={event => save({ rememberPosition: event.target.checked }, !event.target.checked)} />记忆拖拽位置</label>
    <p className="hint">拖拽面板边缘调整大小；缩放光标出现时，双击左右边缘恢复默认宽度，双击上下边缘恢复默认高度，双击角落恢复两维。大小会在下次打开时恢复。</p>
    <ShortcutTable shortcut={s.shortcut} renameShortcut={s.renameShortcut} />
    <section className="cache-cleanup" aria-labelledby="cache-cleanup-title"><div className="cache-cleanup-copy"><h3 id="cache-cleanup-title">清理缓存</h3>
      <p className="hint">删除所有普通记录、对应的本地文件及孤立附件。收藏和自建分组中的记录会保留。当前可清理缓存大小：<span id="cache-size">{state.cacheSizeText}</span>。此操作无法撤销。</p></div>
      <button id="clear-cache" className="primary" disabled={state.clearingCache} onClick={() => { void controller.clearCache(); }}>{state.clearingCache ? "清理中…" : "清理缓存"}</button></section>
    <p className="app-version">当前版本：<span id="app-version">{version}</span></p>
  </>;
}
function ShortcutTable({ shortcut, renameShortcut }: { shortcut: string; renameShortcut: string }) {
  const rows = [
    [shortcut ? displayShortcut(shortcut) : "未设置", "唤出面板（全局快捷键）"],
    [renameShortcut ? displayShortcut(renameShortcut) : "未设置", "重命名当前记录（仅记录列表）"],
    ["直接输入", "搜索框聚焦时搜索当前分组"], ["Ctrl+F", "聚焦搜索框"],
    [isMacOS ? "Delete / Fn+Delete" : "Backspace / Delete", "搜索框聚焦时编辑搜索词"],
    ["Tab / Shift+Tab", "切换到下一个 / 上一个分组"], ["←", "选中图片或视频记录时打开大图预览"], ["→", "显示当前选中记录的悬浮预览"],
    ["↑ / ↓", "选择上一条 / 下一条记录，首尾循环；分组菜单中切换选项"],
    ["Shift+↑ / Shift+↓、Shift+点击", "按当前列表顺序连续选择多条记录"],
    ["Enter", "应用选中记录；分组菜单中确认选择；编辑分组名或普通记录数量上限时提交"],
    ["Space", "打开或关闭大图预览；分组菜单中确认选择"], ["1～9", "搜索框未聚焦时应用当前可见序号对应的记录"],
    ["Ctrl+1～9", "应用当前可见序号对应的记录，搜索框聚焦时也可使用"],
    ["Ctrl+D", "收藏或取消收藏所有选中记录"], [isMacOS ? "Ctrl+Delete" : "Ctrl+Backspace / Delete", "搜索框未聚焦时删除所有选中记录"],
    ["Ctrl+G", "打开分组菜单，移动所有选中记录"], ["Ctrl+P", "置顶或取消置顶所有选中记录"],
    ["Ctrl+A", "输入框聚焦时全选框内文字；面板其他区域禁用全选"],
    ["Esc", "关闭当前菜单、设置或大图预览；搜索词非空时清除搜索词并移走搜索框焦点，面板保持显示；搜索框为空时在主页隐藏面板；编辑分组名时取消"],
  ];
  return <><h3>快捷键</h3><table className="shortcut-table"><thead><tr><th scope="col">按键</th><th scope="col">作用</th></tr></thead><tbody>{rows.map(([key, label]) => <tr key={label}><td>{key}</td><td>{label}</td></tr>)}</tbody></table></>;
}
