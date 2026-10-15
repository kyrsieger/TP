import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { applyTheme } from "./services/theme";
import { defaultTheme } from "./domain/themes";
import { createApplication } from "./application";
import { panelWindow } from "./services/desktop";
import { Icon, IconDefinitions } from "./components/Icon";
import { Toolbar } from "./components/Toolbar";
import { GroupTabs } from "./components/GroupTabs";
import { RecordList } from "./components/RecordList";
import type { ListHandle } from "./services/viewport";
import { PopupMenus } from "./components/PopupMenus";
import { SettingsPanel } from "./components/SettingsPanel";
import { CleanupConfirmation } from "./components/CleanupConfirmation";
import { useObservable } from "./components/use-observable";

export function App() {
  const listRef = useRef<ListHandle>(null);
  const [app] = useState(() => createApplication({
    row: id => listRef.current?.row(id) ?? null, ensure: id => listRef.current?.ensure(id) ?? null,
    visibleRecords: () => listRef.current?.visibleRecords() ?? [], reset: () => listRef.current?.reset(),
    scrollBackToTop: () => listRef.current?.scrollBackToTop(), cancelScroll: () => listRef.current?.cancelScroll(),
  }));
  const panel = useObservable(app.history), settings = useObservable(app.settings), toast = useObservable(app.toasts);
  const [backToTop, setBackToTop] = useState(false);
  const theme = settings.themeDraft ?? panel.settings?.theme ?? defaultTheme;
  useLayoutEffect(() => { applyTheme(theme); void emit("theme-changed", theme).catch(() => {}); }, [theme]);
  useEffect(() => { void app.start(); return () => app.dispose(); }, [app]);
  const groupContextMenu = (event: React.MouseEvent) => {
    const target = event.target as HTMLElement;
    if (target.closest(".group-input")) return;
    event.preventDefault(); app.menus.update({ group: null, record: null });
    const group = target.closest<HTMLElement>(".group[data-view^='group:']");
    if (!group || app.groups.value.saving) return;
    app.groups.cancel(); app.menus.update({ group: { id: Number(group.dataset.view!.slice(6)), x: event.clientX, y: event.clientY } });
  };
  const recordContextMenu = (event: React.MouseEvent) => {
    app.menus.update({ record: null });
    const row = (event.target as HTMLElement).closest<HTMLElement>(".record"), record = panel.snapshot?.records.find(record => record.id === row?.dataset.id);
    if (!record) return;
    event.preventDefault(); app.menus.update({ group: null, move: null });
    if (app.rename.value.saving) return;
    app.rename.cancel(false);
    app.previews.closePreview(); app.menus.update({ record: { id: record.id, x: event.clientX, y: event.clientY, scrollTop: event.currentTarget.scrollTop } });
  };
  return <>
    <div className="shell" onPointerDownCapture={event => {
      const element = event.target as HTMLElement, patch: Partial<typeof app.menus.value> = {};
      if (!element.closest("#record-menu")) patch.record = null;
      if (!element.closest("#group-menu")) patch.group = null;
      if (!element.closest("#filter-wrap")) patch.filter = false;
      if (!element.closest(".retention-select")) patch.retention = false;
      if (!element.closest(".fixed-position-select")) patch.fixedPosition = false;
      if (!element.closest(".theme-select")) patch.theme = false;
      if (!element.closest("#move-group-menu,[data-move-group-trigger]")) patch.move = null;
      app.menus.update(patch);
    }}>
      {["left", "right", "top", "bottom"].map(edge => <div key={edge} className={`resize-edge ${edge}`} data-resize-edge={edge} onPointerDown={event => { void app.geometry.resize(event.nativeEvent, event.currentTarget); }} />)}
      <IconDefinitions />
      <header className="titlebar" onPointerDown={event => { if (event.button === 0 && !(event.target as HTMLElement).closest("button")) void panelWindow.startDragging().catch(error => app.toasts.show(String(error))); }}>
        <svg className="logo" viewBox="0 0 14 14" aria-hidden="true"><rect x="1" y="1" width="12" height="12" rx="3.5" fill="none" stroke="var(--accent)" strokeWidth="2" /><rect x="4.6" y="4.6" width="4.8" height="4.8" rx="1.4" fill="var(--accent)" /></svg><span className="appname">TP</span>
        <div className="winbtns"><button id="minimize" className="winbtn" title="最小化" aria-label="最小化" onClick={() => { void panelWindow.minimize().catch(error => app.toasts.show(String(error))); }}><Icon name="minus" size={13} /></button>
          <button id="maximize" className="winbtn" title="最大化或还原" aria-label="最大化或还原" onClick={() => { void panelWindow.toggleMaximize().catch(error => app.toasts.show(String(error))); }}><Icon name="square" size={11} /></button>
          <button id="close" className="winbtn close" title="隐藏面板" aria-label="隐藏面板" onClick={() => { app.previews.largePreviewId = null; void app.history.action("hide_panel"); }}><Icon name="x" size={13} /></button></div>
      </header>
      <Toolbar history={app.history} menus={app.menus} settingsOpen={settings.open} toggleSettings={() => { if (settings.open) app.closeSettings(); else void app.settings.open(); }}
        clearSearch={() => app.clearSearch()} captureFocus={() => { if (!settings.open) app.focus.capture(); }} />
      <GroupTabs history={app.history} groups={app.groups} backToTop={backToTop} selectGroup={app.selectGroup} scrollBackToTop={() => listRef.current?.scrollBackToTop()} contextMenu={groupContextMenu} />
      <RecordList history={app.history} rename={app.rename} previews={app.previews} menus={app.menus} listRef={listRef} onBackToTop={setBackToTop} onContextMenu={recordContextMenu} />
      <PopupMenus menus={app.menus} history={app.history} groups={app.groups} toast={app.toasts.show} renameRecord={app.startRecordRename} />
      <section id="settings" hidden={!settings.open}>{panel.active && settings.open && <SettingsPanel controller={app.settings} history={app.history} menus={app.menus} toast={app.toasts.show} />}</section>
      <div id="toast" role="status" className={toast.visible ? "visible" : ""}>{toast.message}</div>
    </div>
    {(settings.confirmationCount !== null || settings.cacheConfirmationOpen) && <CleanupConfirmation count={settings.confirmationCount} controller={app.settings} />}
  </>;
}
