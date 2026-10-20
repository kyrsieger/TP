// Browser-only IPC fixture. Loaded explicitly by test pages, never by application entries.
const callbacks = new Map(), listeners = new Map();
let nextId = 1, visible = false, confirmationCount = 0, generation = 0;
const label = new URLSearchParams(location.search).get("window") || "main";
const settings = { maxOrdinary: 300, retentionDays: 7, sensitiveSeconds: 90, shortcut: "", renameShortcut: "F2", positionMode: "fixed", fixedPosition: "right",
  theme: window.__TP_TEST_INITIAL_THEME__ ?? "indigo", fontFamily: window.__TP_TEST_INITIAL_FONT__ ?? "",
  fontStyle: window.__TP_TEST_INITIAL_FONT_STYLE__ ?? { name: "Regular", family: "", weight: 400, stretch: 100, style: "normal" }, backgroundResident: false, retainTypeFilter: true,
  rememberPosition: false, panelWidth: 420, panelHeight: 600, panelX: null, panelY: null, alwaysOnTop: false, retainSearchInput: true, showTaskbarIcon: true };
const makeRecord = (id, kind, body, overrides = {}) => ({ id, kind, body, title: "", files: [], favorite: false, pinnedAt: null, groupId: null,
  usedAt: 1, createdAt: null, expiresAt: null, status: "ready", preview: false, assetPath: null, ...overrides });
const image = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><rect width="320" height="180" fill="#eef0ff"/><circle cx="160" cy="90" r="50" fill="#4f46e5"/></svg>')}`;
const state = { records: [
  makeRecord("text", "text", "React 迁移测试\n第二行", { favorite: true }),
  makeRecord("link", "link", "https://example.com/react"),
  makeRecord("color", "color", "#4f46e5"),
  makeRecord("image", "image", "", { preview: true, assetPath: "C:\\TP\\image.png" }),
  makeRecord("file", "file", "", { files: [{ original: "C:\\Files\\notes.md", stored: "C:\\TP\\notes.md", isDirectory: false }], groupId: 1 }),
  makeRecord("video", "video", "", { preview: true, files: [{ original: "C:\\Files\\video.mp4", stored: null, isDirectory: false }] }),
  ...Array.from({ length: 120 }, (_, index) => makeRecord(`row-${index}`, "text", `记录 ${index + 1}`)),
], groups: [{ id: 1, name: "工作" }, { id: 2, name: "资料" }], settings };
let session = { view: "default", kindFilter: "all", search: "" };
const calls = [];
function emit(event, payload = null) {
  for (const [id, item] of [...listeners]) if (item.event === event) callbacks.get(item.handler)?.({ event, id, payload });
}
window.__TP_TEST__ = {
  state, calls, emit, image, makeRecord,
  setConfirmationCount(count) { confirmationCount = count; },
  get visible() { return visible; }, get generation() { return generation; },
  show() { emit("open-requested", ++generation); },
  hide() { visible = false; emit("panel-hidden", ++generation); },
  idle(token = generation) { emit("panel-idle-release", token); },
  change(records) { state.records = records; emit("history-changed"); },
  listeners,
};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_event, id) => listeners.delete(id) };
window.__TAURI_INTERNALS__ = {
  metadata: { currentWindow: { label }, currentWebview: { label } },
  transformCallback(callback) { const id = nextId++; callbacks.set(id, callback); return id; },
  unregisterCallback(id) { callbacks.delete(id); },
  async invoke(name, args = {}) {
    calls.push({ name, args: structuredClone(args) });
    if (name === "plugin:event|listen") { const id = nextId++; listeners.set(id, args); return id; }
    if (name === "plugin:event|unlisten") { listeners.delete(args.eventId); return; }
    if (name === "plugin:event|emit" || name === "plugin:event|emit_to") {
      if (args.event.endsWith("-ping")) emit(args.event.replace("-ping", "-ready"));
      if (label !== "main") emit(args.event, args.payload);
      return;
    }
    if (name === "get_panel_session") return structuredClone(session);
    if (name === "save_panel_session") { session = { ...args.session }; return; }
    if (name === "panel_ready") { setTimeout(() => window.__TP_TEST__.show(), 0); return null; }
    if (name === "get_state") return structuredClone(state);
    if (name === "get_theme") return state.settings.theme ?? "indigo";
    if (name === "get_font") return { family: state.settings.fontFamily ?? "", style: state.settings.fontStyle };
    if (name === "get_system_fonts") {
      if (window.__TP_TEST__.fontListError) throw new Error("font list unavailable");
      if (window.__TP_TEST__.systemFonts) return window.__TP_TEST__.systemFonts;
      return ["Arial", "Consolas", "微软雅黑", "Segoe UI", "等线", "霞鹜文楷", ...Array.from({ length: 120 }, (_, index) => `Test Font ${String(index).padStart(3, "0")}`)]
        .map(name => ({ name, aliases: name === "微软雅黑" ? ["微软雅黑", "Microsoft YaHei", "Microsoft YaHei Light", "微软雅黑 Light"] : name === "霞鹜文楷" ? [name, "LXGW WenKai"] : [name], styles: [
          { name: "Regular", family: name, weight: 400, stretch: 100, style: "normal", aliases: [name] },
          ...(name === "等线" || name.startsWith("Test Font") ? [] : [
            { name: "Light", family: name === "微软雅黑" ? "微软雅黑 Light" : name, weight: name === "微软雅黑" ? 290 : 300, stretch: 100, style: "normal", aliases: name === "微软雅黑" ? ["Microsoft YaHei Light", "微软雅黑 Light"] : [] },
            { name: "Bold", family: name, weight: 700, stretch: 100, style: "normal", aliases: [name] },
            ...(name === "微软雅黑" ? [] : [{ name: "Italic", family: name, weight: 400, stretch: 100, style: "italic", aliases: [name] }]),
          ]),
        ] }));
    }
    if (name === "show_panel") { visible = true; return; }
    if (name === "hide_panel") { window.__TP_TEST__.hide(); return; }
    if (name === "apply_record") { if (!state.settings.alwaysOnTop) window.__TP_TEST__.hide(); return; }
    if (name === "panel_content_released") {
      window.__TP_TEST__.released = { generation: args.generation, rows: document.querySelectorAll('.record').length, images: document.querySelectorAll('img[src]').length };
      return;
    }
    if (name === "save_settings") {
      if (window.__TP_TEST__.themeSaveDelay) await new Promise(resolve => setTimeout(resolve, window.__TP_TEST__.themeSaveDelay));
      if (window.__TP_TEST__.settingsError) throw new Error(window.__TP_TEST__.settingsError);
      if ((args.value.maxOrdinary !== state.settings.maxOrdinary || args.value.retentionDays !== state.settings.retentionDays) && confirmationCount > (args.confirmedCount ?? 0))
        return { status: "confirmation_required", count: confirmationCount };
      state.settings = { ...args.value }; return { status: "saved" };
    }
    if (name === "set_favorite" || name === "set_record_pinned") {
      const record = state.records.find(record => record.id === args.id);
      if (record) { if (name === "set_favorite") record.favorite = args.favorite; else record.pinnedAt = args.pinned ? Date.now() : null; }
      return;
    }
    if (name === "delete_record") { state.records = state.records.filter(record => record.id !== args.id); return; }
    if (name === "rename_record") {
      if (window.__TP_TEST__.renameError) throw new Error(window.__TP_TEST__.renameError);
      const record = state.records.find(record => record.id === args.id);
      if (!record) throw new Error("记录不存在");
      record.title = args.title.trim(); return;
    }
    if (name === "batch_records") {
      const operation = args.operation;
      if (operation.kind === "delete") state.records = state.records.filter(record => !args.ids.includes(record.id));
      else for (const record of state.records.filter(record => args.ids.includes(record.id))) {
        if (operation.kind === "set_favorite") record.favorite = operation.favorite;
        if (operation.kind === "set_pinned") record.pinnedAt = operation.pinned ? Date.now() : null;
        if (operation.kind === "move_group") record.groupId = operation.groupId;
      }
      return;
    }
    if (name === "add_group") { state.groups.push({ id: Math.max(...state.groups.map(group => group.id), 0) + 1, name: args.name }); return; }
    if (name === "rename_group") { state.groups.find(group => group.id === args.id).name = args.name; return; }
    if (name === "delete_group") { state.groups = state.groups.filter(group => group.id !== args.id); return; }
    if (name === "get_cache_size") return 1536;
    if (name === "clear_cache") { const before = state.records.length; state.records = state.records.filter(record => record.favorite || record.groupId !== null); return before - state.records.length; }
    if (name === "autostart_enabled") return false;
    if (name === "preview_data") return image;
    if (name === "file_text_preview") return "# 文件内容\n中文文本";
    if (name === "release_preview_window") return false;
    if (name === "preview_focused") return false;
    if (name === "plugin:app|version") return "1.0.2";
    if (name === "plugin:window|is_visible") return visible;
    if (name === "plugin:window|scale_factor") return 1;
    if (name === "plugin:window|outer_position") return { x: 1200, y: 200 };
    if (name === "plugin:window|outer_size" || name === "plugin:window|inner_size") return { width: 420, height: 600 };
    if (name === "plugin:window|cursor_position") return { x: 1600, y: 700 };
    if (name === "plugin:window|current_monitor" || name === "plugin:window|monitor_from_point") return {
      name: "Test", size: { width: 1920, height: 1080 }, position: { x: 0, y: 0 }, scaleFactor: 1,
      workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
    };
    return undefined;
  },
};
