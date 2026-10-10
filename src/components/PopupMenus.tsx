import { useLayoutEffect, useRef } from "react";
import { call } from "../services/desktop";
import type { GroupController } from "../state/groups";
import type { MenuController } from "../state/menus";
import type { PanelController } from "../state/panel";
import { useObservable } from "./use-observable";

export function PopupMenus({ menus, history, groups, toast }: { menus: MenuController; history: PanelController; groups: GroupController; toast: (message: string) => void }) {
  const menu = useObservable(menus), panel = useObservable(history);
  const groupRef = useRef<HTMLDivElement>(null), recordRef = useRef<HTMLDivElement>(null), moveRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const shell = groupRef.current!.closest<HTMLElement>(".shell")!, bounds = shell.getBoundingClientRect();
    for (const [value, element] of [[menu.group, groupRef.current], [menu.record, recordRef.current]] as const) {
      if (!value || !element) continue;
      element.style.left = `${Math.max(8, Math.min(value.x - bounds.left, shell.clientWidth - element.offsetWidth - 8))}px`;
      element.style.top = `${Math.max(8, Math.min(value.y - bounds.top, shell.clientHeight - element.offsetHeight - 8))}px`;
    }
    if (menu.record) recordRef.current!.querySelector("button")!.focus();
    if (menu.move) {
      const element = moveRef.current!, anchor = menu.move.anchor;
      element.style.left = `${Math.max(8, Math.min(anchor.right - bounds.left - element.offsetWidth, shell.clientWidth - element.offsetWidth - 8))}px`;
      const below = anchor.bottom - bounds.top + 4, above = anchor.top - bounds.top - element.offsetHeight - 4;
      element.style.top = `${below + element.offsetHeight <= shell.clientHeight - 8 ? below : Math.max(8, above)}px`;
      (element.querySelector<HTMLButtonElement>("[aria-checked='true']") ?? element.querySelector<HTMLButtonElement>("button"))?.focus();
    }
  }, [menu.group, menu.record, menu.move]);
  const groupAction = async (rename: boolean) => {
    const id = menu.group?.id;
    if (id === undefined) return;
    menus.update({ group: null });
    if (rename) { groups.start("rename", id); return; }
    try { await call("delete_group", { id }); await history.refresh(); }
    catch (error) { toast(String(error)); }
  };
  const recordAction = async (name: string) => {
    const id = menu.record?.id;
    menus.update({ record: null });
    try { await call(name, { id }); } catch (error) { toast(String(error)); }
  };
  const moveRecords = panel.snapshot?.records.filter(record => menu.move?.ids.includes(record.id)) ?? [];
  return <>
    <div id="group-menu" ref={groupRef} role="menu" hidden={!menu.group}>
      <button type="button" data-group-action="rename" role="menuitem" onClick={() => { void groupAction(true); }}>重命名</button>
      <button type="button" data-group-action="delete" role="menuitem" onClick={() => { void groupAction(false); }}>删除</button>
    </div>
    <div id="record-menu" ref={recordRef} role="menu" aria-label="记录操作" hidden={!menu.record} data-record-id={menu.record?.id}>
      {[["copy-paths", "复制路径", "copy_record_paths"], ["open-directory", "打开所在目录", "open_record_directory"], ["save-as", "另存为", "save_record_files"]].map(([id, label, command]) =>
        <button key={id} type="button" data-record-action={id} role="menuitem" onClick={() => { void recordAction(command); }}>{label}</button>)}
    </div>
    <div id="move-group-menu" ref={moveRef} role="menu" aria-label="移动到分组" hidden={!menu.move} data-record-id={menu.move?.triggerId}>
      {menu.move && [{ id: null, name: "未分组" }, ...(panel.snapshot?.groups ?? [])].map(group => <button key={group.id ?? "none"} type="button" role="menuitemradio" data-move-group={group.id ?? ""}
        aria-checked={moveRecords.every(record => record.groupId === group.id)} onMouseEnter={event => event.currentTarget.focus()} onClick={() => {
          const ids = menu.move!.ids; menus.update({ move: null });
          if (moveRecords.some(record => record.groupId !== group.id)) void history.action("batch_records", { ids, operation: { kind: "move_group", groupId: group.id } });
        }}><span className="move-group-name">{group.name}</span><span className="move-group-check" aria-hidden="true">✓</span></button>)}
    </div>
  </>;
}
