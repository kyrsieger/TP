import { useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type Ref } from "react";
import { flushSync } from "react-dom";
import { backToTopOffset, listOffsets, rowScrollTop, visibleRange } from "../list-layout";
import type { PanelController } from "../state/panel";
import type { PreviewController } from "../services/previews";
import type { MenuController } from "../state/menus";
import type { RecordRenameController } from "../state/record-rename";
import { RecordRow } from "./RecordRow";
import { useObservable } from "./use-observable";
import type { ListHandle } from "../services/viewport";
export function RecordList({ history, rename, previews, menus, listRef, onBackToTop, onContextMenu }:
  { history: PanelController; rename: RecordRenameController; previews: PreviewController; menus: MenuController; listRef: Ref<ListHandle>;
    onBackToTop: (show: boolean) => void; onContextMenu: (event: React.MouseEvent) => void }) {
  const state = useObservable(history);
  const menu = useObservable(menus);
  const edit = useObservable(rename);
  const ref = useRef<HTMLElement>(null);
  const frame = useRef(0);
  const [metrics, setMetrics] = useState({ scrollTop: 0, height: 480 });
  const [ordinals, setOrdinals] = useState<string[]>([]);
  const records = useMemo(() => history.records, [history, state.snapshot, state.view, state.kindFilter, state.search]);
  const offsets = useMemo(() => listOffsets(records.map(record => record.id === edit.id && (record.kind === "image" || record.kind === "video")
    ? { ...record, title: record.title || " " } : record)), [records, edit.id]);
  const range = visibleRange(offsets, Math.max(0, metrics.scrollTop - 2), metrics.height - 8);
  const cancelScroll = () => { cancelAnimationFrame(frame.current); frame.current = 0; };
  function row(id: string) { return [...(ref.current?.querySelectorAll<HTMLElement>(".record") ?? [])].find(row => row.dataset.id === id) ?? null; }
  function visibleRecords() {
    const list = ref.current!;
    const bounds = list.getBoundingClientRect();
    return [...list.querySelectorAll<HTMLElement>(".record")].filter(row => {
      const rect = row.getBoundingClientRect();
      return rect.bottom > bounds.top + 1 && rect.top < bounds.bottom - 1;
    });
  }
  function measure() {
    const list = ref.current!;
    const next = { scrollTop: list.scrollTop, height: list.clientHeight };
    setMetrics(previous => previous.scrollTop === next.scrollTop && previous.height === next.height ? previous : next);
    const style = getComputedStyle(list);
    const threshold = backToTopOffset(offsets, list.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom));
    onBackToTop(threshold !== null && threshold + 2 - list.scrollTop < list.clientHeight);
  }
  useImperativeHandle(listRef, () => ({
    row, visibleRecords, cancelScroll,
    ensure(id) {
      const index = records.findIndex(record => record.id === id);
      if (index < 0) return null;
      const list = ref.current!;
      list.scrollTop = rowScrollTop(offsets, index, list.scrollTop, list.clientHeight - 8);
      flushSync(measure);
      return row(id);
    },
    reset() { cancelScroll(); ref.current!.scrollTop = 0; measure(); },
    scrollBackToTop() {
      const list = ref.current!;
      cancelScroll();
      const start = list.scrollTop;
      if (matchMedia("(prefers-reduced-motion: reduce)").matches) { list.scrollTop = 0; measure(); return; }
      const duration = Math.min(700, 320 + Math.sqrt(start) * 12), startedAt = performance.now();
      const step = (now: number) => {
        const progress = Math.min(1, (now - startedAt) / duration);
        const eased = progress < .5 ? 2 * progress * progress : 1 - 2 * (1 - progress) * (1 - progress);
        list.scrollTop = start * (1 - eased);
        if (progress < 1) frame.current = requestAnimationFrame(step); else frame.current = 0;
      };
      frame.current = requestAnimationFrame(step);
    },
  }));
  useLayoutEffect(() => {
    const list = ref.current!;
    const observer = new ResizeObserver(measure);
    observer.observe(list);
    return () => { observer.disconnect(); cancelScroll(); };
  }, [offsets]);
  useLayoutEffect(() => {
    const list = ref.current!;
    list.scrollTop = Math.min(list.scrollTop, Math.max(0, offsets[offsets.length - 1] + 8 - list.clientHeight));
    measure();
    const ids = visibleRecords().slice(0, 9).map(row => row.dataset.id!);
    setOrdinals(previous => previous.join("\0") === ids.join("\0") ? previous : ids);
  }, [offsets, range.start, range.end, metrics.scrollTop, metrics.height]);
  return <main id="list" ref={ref} onContextMenu={onContextMenu}
    onScroll={() => {
      measure();
      // 开菜单之前发生的滚动可能延迟通知；只有打开后位置确实变化才关闭。
      const context = menus.value.record;
      menus.update({ move: null, record: context?.scrollTop === ref.current!.scrollTop ? context : null });
      previews.closePreview();
    }}
    onWheel={cancelScroll} onPointerDown={cancelScroll}
    onMouseDown={event => { if (event.button === 0 && event.shiftKey && (event.target as HTMLElement).closest(".record") && !(event.target as HTMLElement).closest("button,input")) event.preventDefault(); }}>
    {state.active && (records.length ? <>
      <div className="list-spacer" style={{ height: offsets[range.start] }} aria-hidden="true" />
      {records.slice(range.start, range.end).map((record, i) => <RecordRow key={record.id} record={record} index={range.start + i} last={range.start + i === records.length - 1}
        selected={state.selectedIds.has(record.id)} beforeSelected={state.selectedIds.has(records[range.start + i + 1]?.id ?? "")}
        ordinal={ordinals.includes(record.id) ? String(ordinals.indexOf(record.id) + 1) : ""} search={state.search} root={ref.current}
        moveOpen={menu.move?.triggerId === record.id} select={extend => history.selectRecord(record.id, extend)} apply={() => { void history.apply(record.id); }}
        editing={edit.id === record.id} rename={rename}
        action={(name, args) => { void history.action(name, args); }}
        move={trigger => { if (menu.move?.triggerId === record.id) menus.update({ move: null }); else menus.openMove(trigger, trigger, [record]); }}
        hover={() => { if (!edit.id && !menu.record) previews.hoverRecord(record.id); }} leave={() => previews.leave(record.id)} />)}
      <div className="list-spacer" style={{ height: offsets[records.length] - offsets[range.end] }} aria-hidden="true" />
    </> : <div className="empty">这里还没有记录</div>)}
  </main>;
}
