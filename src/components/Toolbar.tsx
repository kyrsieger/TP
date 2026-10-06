import { labels } from "../domain/records";
import type { Kind } from "../domain/types";
import type { PanelController } from "../state/panel";
import type { MenuController } from "../state/menus";
import { Icon } from "./Icon";
import { useObservable } from "./use-observable";

export function Toolbar({ history, menus, settingsOpen, toggleSettings, clearSearch, captureFocus }: {
  history: PanelController; menus: MenuController; settingsOpen: boolean; toggleSettings: () => void;
  clearSearch: () => void; captureFocus: () => void;
}) {
  const state = useObservable(history);
  const menu = useObservable(menus);
  const disabled = state.view !== "default";
  const selected = !disabled && state.kindFilter !== "all";
  const title = disabled ? "仅默认分组可筛选" : selected ? `类型筛选：${labels[state.kindFilter as Kind]}` : "类型筛选：全部";
  const pinned = state.settings?.alwaysOnTop ?? false;
  const filter = (kindFilter: string) => { menus.update({ filter: false }); history.browse({ kindFilter }); };
  return <div className="toolbar">
    <div className="search-box"><label className="search-field" htmlFor="search"><Icon name="search" size={13} /><input id="search" placeholder="搜索 · Ctrl+F" aria-label="搜索当前分组" autoComplete="off"
      value={state.search} onChange={event => history.browse({ search: event.target.value })} /></label>
      <button id="clear-search" type="button" title="清空搜索" aria-label="清空搜索" hidden={!state.search} onClick={clearSearch}><Icon name="x" size={13} /></button></div>
    <div id="filter-wrap" className={`filter-wrap ${selected ? "active" : ""} ${disabled ? "disabled" : ""}`}><div className="filter-control">
      <button id="kind-filter" className={`iconbtn ${selected ? "active" : ""}`} disabled={disabled} title={title} aria-label={title} aria-haspopup="menu" aria-expanded={menu.filter}
        onClick={() => menus.update({ filter: !menu.filter })}><Icon name={selected ? state.kindFilter : "filter"} /></button>
      <button id="clear-filter" className="clear-filter" title="清除筛选" aria-label="清除筛选" hidden={!selected} onClick={() => filter("all")}><Icon name="x" size={11} /></button></div>
      <div id="filter-menu" className="filter-menu" role="menu" hidden={!menu.filter}>{(["all", "text", "image", "video", "file", "link", "color"] as const).map(kind =>
        <button key={kind} data-kind={kind} role="menuitem" className={kind === state.kindFilter ? "active" : ""} onClick={() => filter(kind)}><Icon name={kind === "all" ? "filter" : kind} /><span>{kind === "all" ? "全部" : labels[kind]}</span></button>)}</div>
    </div>
    <button id="pin" className="iconbtn" aria-label={pinned ? "取消置顶" : "置顶"} aria-pressed={pinned} title={pinned ? "取消置顶" : "置顶"}
      onClick={() => { if (state.settings) void history.action("save_settings", { value: { ...state.settings, alwaysOnTop: !pinned } }); }}><Icon name="pin" size={15} /></button>
    <button id="settings-button" className={`iconbtn ${settingsOpen ? "active" : ""}`} aria-label="设置" title="设置" onPointerDown={captureFocus} onClick={toggleSettings}><Icon name="gear" size={15} /></button>
  </div>;
}
