import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { flushSync } from "react-dom";
import { call, isMacOS, panelWindow } from "./services/desktop";
import { GeometryController } from "./services/geometry";
import { PreviewController } from "./services/previews";
import { SettingsFocus } from "./services/focus";
import { subscribeAll } from "./services/subscriptions";
import { GroupController } from "./state/groups";
import { RecordRenameController } from "./state/record-rename";
import { MenuController } from "./state/menus";
import { PanelController } from "./state/panel";
import { SettingsController } from "./state/settings";
import { ToastController } from "./state/toast";
import { TransferController } from "./state/transfer";
import type { PanelSession } from "./domain/types";
import type { ListHandle } from "./services/viewport";
import { createPanelKeydown, selectAllKeydown } from "./input/keyboard";

/** Composition root: services depend on domain/state, never on mounted components. */
export function createApplication(list: ListHandle) {
  const toasts = new ToastController(), menus = new MenuController(), focus = new SettingsFocus();
  const toast = toasts.show;
  const history = new PanelController(call, {
    reset: () => { previews.reset(); menus.reset(); groups.reset(); rename.reset(); settings.reset(); transfer.reset(); focus.reset(); list.reset(); },
    beforeRefresh: () => { previews.closePreview(); menus.update({ record: null, move: null }); list.cancelScroll(); },
    afterRefresh: () => { previews.reconcile(); rename.reconcile(); if (settings.value.open) void settings.refreshCacheSize(); },
    afterBrowse: () => { rename.cancel(false); previews.reconcile(); },
    place: async () => { settings.syncShortcut(); await geometry.placePanel(); },
    afterOpen: () => {
      const id = history.value.selectedId;
      if (id) list.ensure(id)?.focus({ preventScroll: true });
    },
    isVisible: () => panelWindow.isVisible(), error: toast,
  });
  const groups = new GroupController(history, call, toast);
  const rename = new RecordRenameController(history, call, toast, id => list.row(id)?.focus({ preventScroll: true }));
  const settings = new SettingsController(history, call, { toast, place: () => geometry.placePanel(), position: () => panelWindow.outerPosition() });
  const transfer = new TransferController(history, call, toast, task => settings.runDataTask(task));
  const geometry = new GeometryController(history, panelWindow, toast);
  const previews = new PreviewController(history, panelWindow, list, toast);
  function clearSearch(refocus = true) {
    flushSync(() => history.browse({ search: "" }));
    const input = document.querySelector<HTMLInputElement>("#search")!;
    if (refocus) input.focus({ preventScroll: true }); else input.blur();
  }
  function selectGroup(view: string) {
    menus.update({ filter: view === "default" && menus.value.filter });
    flushSync(() => history.selectGroup(view)); list.reset();
  }
  function cycleGroup(direction: number) {
    flushSync(() => history.cycleGroup(direction));
    if (history.value.view !== "default") menus.update({ filter: false });
    list.reset();
    const active = document.querySelector<HTMLElement>("#groups .group.active");
    active?.scrollIntoView({ block: "nearest", inline: "nearest" }); active?.focus({ preventScroll: true });
  }
  function moveSelection(direction: number, extend = false) {
    list.cancelScroll(); history.moveSelection(direction, extend);
    if (previews.previewFromKeyboard) previews.closePreview();
    const id = history.value.selectedId;
    if (!id) return;
    const row = list.ensure(id); row?.scrollIntoView({ block: "nearest" }); row?.focus({ preventScroll: true });
  }
  function closeSettings() { settings.flushTheme(); menus.update({ theme: false, font: false, fontStyle: false }); flushSync(() => settings.update({ open: false })); focus.restore(list); }
  function startRecordRename(id: string) {
    groups.cancel(); menus.closePopups(); previews.closePreview();
    list.ensure(id);
    flushSync(() => rename.start(id));
  }
  const keydown = createPanelKeydown({ history, settings, menus, previews, list, clearSearch, cycleGroup, moveSelection, closeSettings, startRecordRename, isMacOS });
  let disposed = false;
  let startGeneration = 0;
  let blurTimer: number | undefined;
  const disposers: (() => void)[] = [];
  const onBlur = () => {
    groups.cancel(); rename.cancel(false); menus.closePopups();
    window.clearTimeout(blurTimer);
    blurTimer = window.setTimeout(() => { if (!disposed) void call<boolean>("preview_focused").catch(() => false).then(focused => { if (!disposed && !focused) previews.closePreview(); }); }, 120);
  };
  const onResize = () => { menus.update({ move: null, record: null }); previews.closePreview(); };
  async function start() {
    const generation = ++startGeneration;
    disposed = false;
    const track = (dispose: UnlistenFn) => {
      if (disposed || generation !== startGeneration) dispose(); else disposers.push(dispose);
    };
    document.addEventListener("keydown", selectAllKeydown, true);
    document.addEventListener("keydown", keydown);
    window.addEventListener("blur", onBlur); window.addEventListener("resize", onResize);
    disposers.push(() => {
      document.removeEventListener("keydown", selectAllKeydown, true); document.removeEventListener("keydown", keydown);
      window.removeEventListener("blur", onBlur); window.removeEventListener("resize", onResize);
    });
    try {
      const [session] = await Promise.all([
        call<PanelSession>("get_panel_session"),
        subscribeAll([
          previews.start(),
          listen("history-changed", () => { void history.refresh().catch(error => toast(String(error))); }),
          listen<number>("panel-hidden", event => history.hide(event.payload)),
          listen<number>("panel-idle-release", event => {
            let released = false;
            flushSync(() => { released = history.release(event.payload); });
            if (released) void call("panel_content_released", { generation: event.payload }).catch(error => toast(String(error)));
          }),
          listen<number>("open-requested", event => { void history.open(event.payload).catch(error => toast(String(error))); }),
          panelWindow.onMoved(() => { previews.closePreview(); geometry.scheduleGeometrySave(); }),
          panelWindow.onResized(() => { previews.closePreview(); geometry.scheduleGeometrySave(); }),
        ]).then(track),
      ]);
      if (disposed || generation !== startGeneration) return;
      history.restore(session);
      const error = await call<string | null>("panel_ready");
      if (!disposed && generation === startGeneration) settings.ready(error);
    } catch (error) { if (!disposed && generation === startGeneration) toast(`启动失败：${String(error)}`); }
  }
  return { history, groups, rename, settings, transfer, menus, geometry, previews, focus, toasts, clearSearch, selectGroup, closeSettings, startRecordRename, start,
    dispose() { disposed = true; startGeneration++; window.clearTimeout(blurTimer); disposers.splice(0).forEach(dispose => dispose()); geometry.dispose(); settings.reset(); transfer.reset(); toasts.dispose(); } };
}
export type Application = ReturnType<typeof createApplication>;
