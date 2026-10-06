import type { KeyboardContext } from "./types";
export function createPanelKeydown(context: KeyboardContext) {
  return (event: KeyboardEvent) => {
    const { history, previews, menus, settings, list, clearSearch, cycleGroup, closeSettings, moveSelection, isMacOS } = context;
    const state = history.value.snapshot, selectedId = history.value.selectedId, selectedIds = history.value.selectedIds;
    const panelGeneration = history.generation, settingsOpen = settings.value.open;
    const closeRecordMenu = () => menus.update({ record: null });
    const closeGroupMenu = () => menus.update({ group: null });
    const closeMoveGroupMenu = () => menus.update({ move: null });
    const setFilterMenuOpen = (filter: boolean) => menus.update({ filter });
    const setRetentionMenuOpen = (retention: boolean) => menus.update({ retention });
    const setFixedPositionMenuOpen = (fixedPosition: boolean) => menus.update({ fixedPosition });
    const filtered = () => history.records;
    const apply = (id: string) => history.apply(id);
    const action = (name: string, args?: Record<string, unknown>) => history.action(name, args);
    const visibleRecords = () => list.visibleRecords();
    const recordRow = (id: string) => list.row(id);
    const ensureRecordRow = (id: string) => list.ensure(id);
    const openMoveGroupMenu = (trigger: HTMLElement, anchor: HTMLElement, records: import("../domain/types").RecordItem[]) => menus.openMove(trigger, anchor, records);
    const openLargePreview = (id: string) => previews.openLargePreview(id);
    const showHoverPreview = (id: string) => previews.showHoverPreview(id);
    if (!state || (event.target as HTMLElement).id === "shortcut")
      return;
    if (document.querySelector<HTMLDialogElement>("#settings-cleanup-confirmation")?.open)
      return;
    const recordMenu = document.querySelector<HTMLElement>("#record-menu")!;
    if (!recordMenu.hidden && recordMenu.contains(event.target as Node)) {
      if (event.key === "Escape") {
        event.preventDefault();
        const row = document.querySelector<HTMLElement>(`.record[data-id="${recordMenu.dataset.recordId}"]`);
        closeRecordMenu();
        row?.focus();
        return;
      }
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        (event.target as HTMLButtonElement).click();
        return;
      }
      closeRecordMenu();
    }
    if ((event.target as HTMLElement).matches("[data-move-group-trigger]") && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      (event.target as HTMLButtonElement).click();
      return;
    }
    const moveMenu = document.querySelector<HTMLElement>("#move-group-menu")!;
    if (!moveMenu.hidden && moveMenu.contains(event.target as Node)) {
      if (event.key === "Escape") {
        event.preventDefault();
        const trigger = document.querySelector<HTMLElement>(`[data-move-group-trigger="${moveMenu.dataset.recordId}"]`);
        closeMoveGroupMenu();
        trigger?.focus();
      }
      else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
        event.preventDefault();
        const options = [...moveMenu.querySelectorAll<HTMLButtonElement>("button")];
        const index = options.indexOf(event.target as HTMLButtonElement);
        options[(index + (event.key === "ArrowUp" ? -1 : 1) + options.length) % options.length].focus();
      }
      else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        (event.target as HTMLButtonElement).click();
      }
      else if (event.key === "Tab") {
        event.preventDefault();
        closeMoveGroupMenu();
        cycleGroup(event.shiftKey ? -1 : 1);
      }
      return;
    }
    if (event.key === "Escape" && !document.querySelector<HTMLElement>("#group-menu")!.hidden) {
      event.preventDefault();
      closeGroupMenu();
      return;
    }
    const filterMenu = document.querySelector<HTMLElement>("#filter-menu")!;
    if (event.key === "Escape" && !filterMenu.hidden) {
      event.preventDefault();
      setFilterMenuOpen(false);
      return;
    }
    if (settingsOpen) {
      if (event.key === "Escape") {
        const retentionMenu = document.querySelector<HTMLElement>("#retention-menu");
        const fixedPositionMenu = document.querySelector<HTMLElement>("#fixed-position-menu");
        if (retentionMenu && !retentionMenu.hidden)
          setRetentionMenuOpen(false);
        else if (fixedPositionMenu && !fixedPositionMenu.hidden)
          setFixedPositionMenuOpen(false);
        else
          closeSettings();
      }
      return;
    }
    if (event.isComposing || event.keyCode === 229)
      return;
    if (event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey && !event.getModifierState("AltGraph") && event.code === "KeyF") {
      event.preventDefault();
      document.querySelector<HTMLInputElement>("#search")!.focus();
      return;
    }
    if (/^[1-9]$/.test(event.key) && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey && !event.getModifierState("AltGraph") &&
      !(event.target as HTMLElement).closest("input, textarea, [contenteditable='true']")) {
      event.preventDefault();
      const id = visibleRecords()[Number(event.key) - 1]?.dataset.id;
      if (!event.repeat && id)
        void apply(id);
      return;
    }
    if (event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey && !event.getModifierState("AltGraph") &&
      ["KeyD", "KeyG", "KeyP"].includes(event.code)) {
      event.preventDefault();
      if (event.repeat || !selectedId)
        return;
      const records = filtered().filter(record => selectedIds.has(record.id));
      if (!records.length)
        return;
      const ids = records.map(record => record.id);
      if (event.code === "KeyD")
        void action("batch_records", { ids, operation: { kind: "set_favorite", favorite: !records.every(record => record.favorite) } });
      if (event.code === "KeyP")
        void action("batch_records", { ids, operation: { kind: "set_pinned", pinned: !records.every(record => record.pinnedAt !== null) } });
      if (event.code === "KeyG") {
        const id = selectedId;
        const generation = panelGeneration;
        ensureRecordRow(id);
        window.requestAnimationFrame(() => {
          if (!history.value.active || history.generation !== generation || history.value.selectedId !== id)
            return;
          const row = recordRow(id);
          const trigger = row?.querySelector<HTMLElement>("[data-move-group-trigger]");
          if (row && trigger)
            openMoveGroupMenu(trigger, row, records);
        });
      }
      return;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      cycleGroup(event.shiftKey ? -1 : 1);
      return;
    }
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      document.querySelector<HTMLInputElement>("#search")!.blur();
      moveSelection(event.key === "ArrowUp" ? -1 : 1, event.shiftKey);
      return;
    }
    // 左右方向键不再切换分组：左键打开大图预览，右键显示悬浮预览。输入类元素内保留原生按键行为。
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      if ((event.target as HTMLElement).closest("input, textarea, [contenteditable='true']"))
        return;
      event.preventDefault();
      if (!selectedId || event.repeat)
        return;
      const record = state.records.find(item => item.id === selectedId);
      if (event.key === "ArrowLeft") {
        if (record?.kind === "image" || record?.kind === "video")
          void openLargePreview(selectedId);
      }
      else if (record)
        showHoverPreview(selectedId);
      return;
    }
    if (event.code === "Space" && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey &&
      !(event.target as HTMLElement).closest("input, textarea, button, [contenteditable='true']") && selectedId) {
      const record = state.records.find(item => item.id === selectedId);
      if (record?.kind === "image" || record?.kind === "video") {
        event.preventDefault();
        if (!event.repeat)
          void openLargePreview(selectedId);
        return;
      }
    }
    if (event.key === "Enter" && selectedId) {
      event.preventDefault();
      void apply(selectedId);
      return;
    }
    if (event.key === "Escape") {
      // 搜索框有内容时，Esc 先清空搜索词、移走焦点并保留面板；搜索框为空才隐藏面板。
      if (document.querySelector<HTMLInputElement>("#search")!.value !== "") {
        event.preventDefault();
        clearSearch(false);
        return;
      }
      previews.largePreviewId = null;
      void action("hide_panel");
      return;
    }
    // Mac 主键盘上标为 Delete 的退格键也发送 Backspace；仅 Windows 支持单独按向前删除键删除记录。
    if ((event.key === "Backspace" && event.ctrlKey || !isMacOS && event.key === "Delete" && !event.ctrlKey) &&
      !event.altKey && !event.metaKey && !event.shiftKey && !event.getModifierState("AltGraph") &&
      !(event.target as HTMLElement).closest("input, textarea, [contenteditable='true']")) {
      event.preventDefault();
      if (!event.repeat && selectedIds.size)
        void action("batch_records", { ids: [...selectedIds], operation: { kind: "delete" } });
      return;
    }
  };
}
export function selectAllKeydown(event: KeyboardEvent) {
  if (!event.ctrlKey || event.altKey || event.metaKey || event.shiftKey || event.getModifierState("AltGraph") || event.key.toLowerCase() !== "a")
    return;
  const target = event.target;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)
    target.select();
  event.preventDefault();
  event.stopImmediatePropagation();
}
