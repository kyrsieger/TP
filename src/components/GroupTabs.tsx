import { useLayoutEffect, useRef, useState } from "react";
import type { GroupController } from "../state/groups";
import type { PanelController } from "../state/panel";
import { Icon } from "./Icon";
import { useObservable } from "./use-observable";
import { useGroupDrag } from "./use-group-drag";

function GroupTab({ id, name, active, backToTop, select, edit, dragging, dropBefore, offset }: {
  id: string; name: string; active: boolean; backToTop: boolean; select: () => void; edit: () => void;
  dragging: boolean; dropBefore: boolean; offset: number;
}) {
  const ref = useRef<HTMLButtonElement>(null);
  const [width, setWidth] = useState<number>();
  useLayoutEffect(() => {
    if (!backToTop) setWidth(undefined);
    else if (width === undefined) setWidth(ref.current!.getBoundingClientRect().width);
  }, [backToTop, width]);
  return <button ref={ref} className={`group ${active ? "active" : ""} ${backToTop ? "back-to-top" : ""} ${dragging ? "dragging" : ""} ${dropBefore ? "drop-before" : ""}`}
    data-view={id} data-label={name} style={{ width, transform: dragging ? `translateX(${offset}px)` : undefined }} title={backToTop ? "返回顶部" : undefined} aria-label={backToTop ? "返回顶部" : undefined}
    onClick={select} onDoubleClick={() => { if (!backToTop) edit(); }}>{backToTop && width !== undefined ? <Icon name="arrow-up-to-line" size={15} /> : name}</button>;
}
export function GroupTabs({ history, groups, backToTop, selectGroup, scrollBackToTop, contextMenu }: {
  history: PanelController; groups: GroupController; backToTop: boolean;
  selectGroup: (view: string) => void; scrollBackToTop: () => void; contextMenu: (event: React.MouseEvent) => void;
}) {
  const state = useObservable(history);
  const editing = useObservable(groups);
  const ref = useRef<HTMLElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const { drag, handlers } = useGroupDrag(ref, state.active && !editing.edit && !editing.saving,
    (state.snapshot?.groups ?? []).map(group => group.id), ids => { void groups.reorder(ids); });
  useLayoutEffect(() => {
    input.current?.focus();
    if (editing.edit?.mode === "rename") input.current?.select();
  }, [editing.edit, editing.focusRevision]);
  useLayoutEffect(() => {
    const tabs = ref.current!;
    const wheel = (event: WheelEvent) => {
      if (event.shiftKey || !event.deltaY || tabs.scrollWidth <= tabs.clientWidth) return;
      event.preventDefault();
      tabs.scrollLeft += event.deltaY * (event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? tabs.clientWidth : 1);
    };
    tabs.addEventListener("wheel", wheel, { passive: false });
    return () => tabs.removeEventListener("wheel", wheel);
  }, []);
  const groupInput = <input ref={input} className="group-input" maxLength={40} aria-label={editing.edit?.mode === "create" ? "新分组名称" : "重命名分组"}
    placeholder={editing.edit?.mode === "create" ? "分组名称" : ""} value={editing.name} onChange={event => groups.update({ name: event.target.value })}
    onBlur={event => { const element = event.currentTarget; setTimeout(() => { if (document.activeElement !== element) groups.cancel(); }, 0); }}
    onKeyDown={event => { event.stopPropagation(); if (event.key === "Enter") { event.preventDefault(); void groups.commit(); } if (event.key === "Escape") { event.preventDefault(); groups.cancel(); } }} />;
  return <nav id="groups" ref={ref} aria-label="分组" className={drag ? "is-dragging" : undefined} {...handlers} onContextMenu={contextMenu}>
    {state.snapshot !== null && <>{[{ id: "default", name: "默认" }, { id: "favorite", name: "收藏" }, ...(state.snapshot?.groups ?? []).map(group => ({ id: `group:${group.id}`, name: group.name }))]
      .map(group => editing.edit?.mode === "rename" && group.id === `group:${editing.edit.id}` ? <span key={group.id} style={{ display: "contents" }}>{groupInput}</span>
        : <GroupTab key={group.id} {...group} active={state.view === group.id} backToTop={backToTop && state.view === group.id}
          dragging={group.id === `group:${drag?.id}`} dropBefore={!!drag && group.id === `group:${drag.before}`} offset={drag?.offset ?? 0}
          select={() => { if (backToTop && state.view === group.id) scrollBackToTop(); else selectGroup(group.id); }}
          edit={() => { if (group.id.startsWith("group:")) groups.start("rename", Number(group.id.slice(6))); }} />)}
      {editing.edit?.mode === "create" ? groupInput : <button className={`add-group ${drag && drag.before === null ? "drop-before" : ""}`} title="添加分组" aria-label="添加分组" id="add-group" onClick={() => groups.start("create")}><Icon name="plus" /></button>}</>}
  </nav>;
}
