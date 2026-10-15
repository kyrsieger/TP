import { useEffect, useRef, useState } from "react";
import { labels, fileTagText, recordListText } from "../domain/records";
import type { RecordItem } from "../domain/types";
import { call } from "../services/desktop";
import { Icon } from "./Icon";
import { Highlight } from "./Highlight";
import type { RecordRenameController } from "../state/record-rename";

function RecordTitle({ record, editing, rename, search }: { record: RecordItem; editing: boolean; rename: RecordRenameController; search: string }) {
  const ref = useRef<HTMLInputElement>(null);
  const edit = rename.value;
  useEffect(() => { if (editing) { ref.current?.focus({ preventScroll: true }); ref.current?.select(); } }, [editing, edit.focusRevision]);
  return editing ? <input ref={ref} className="record-rename-input" aria-label="记录标题" placeholder="输入标题" value={edit.title} disabled={edit.saving}
    onChange={event => rename.update({ title: event.target.value })} onBlur={() => rename.cancel(false)}
    onKeyDown={event => {
      event.stopPropagation();
      if (event.nativeEvent.isComposing || event.keyCode === 229) return;
      if (event.key === "Enter") { event.preventDefault(); void rename.commit(); }
      if (event.key === "Escape") { event.preventDefault(); rename.cancel(); }
    }} /> : <span className="record-title" title={record.title}><Highlight text={record.title} query={search} /></span>;
}

function Thumbnail({ record, root }: { record: RecordItem; root: HTMLElement | null }) {
  const ref = useRef<HTMLImageElement>(null);
  const [src, setSrc] = useState<string>();
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    const image = ref.current;
    if (!image) return;
    let alive = true;
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      observer.disconnect();
      void call<string>("preview_data", { id: record.id, role: "thumbnail" }).then(value => {
        if (alive) setSrc(value);
      }).catch(() => { if (alive) setFailed(true); });
    }, { root, rootMargin: "180px 0px" });
    observer.observe(image);
    return () => { alive = false; observer.disconnect(); image.removeAttribute("src"); };
  }, [record.id, root]);
  return failed ? <span className="preview-unavailable">预览图不可用</span>
    : <><img ref={ref} className="preview" data-preview={record.id} src={src} alt={`${labels[record.kind]}预览`} />
      {record.kind === "video" && <span className="play"><Icon name="play" size={10} fill="currentColor" /></span>}</>;
}
type Props = {
  record: RecordItem; index: number; last: boolean; selected: boolean; beforeSelected: boolean;
  ordinal: string; search: string; root: HTMLElement | null; moveOpen: boolean;
  select: (extend: boolean) => void; apply: () => void;
  action: (name: string, args: Record<string, unknown>) => void;
  move: (trigger: HTMLElement) => void; hover: () => void; leave: () => void;
  editing: boolean; rename: RecordRenameController;
};
export function RecordRow({ record, index, last, selected, beforeSelected, ordinal, search, root, moveOpen,
  select, apply, action, move, hover, leave, editing, rename }: Props) {
  const status = record.status === "pending" ? "保存副本中" : record.status === "missing" ? "文件已不可用" : record.status === "copy_failed" ? "副本保存失败" : "";
  const swatch = record.kind === "color" && /^#(?:[\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8})$/i.test(record.body.trim());
  const media = record.kind === "image" || record.kind === "video";
  const hasPreview = record.preview || (record.kind === "image" && record.files.length > 0 && record.status !== "pending");
  const text = media && !search ? "" : recordListText(record, search);
  const fileTag = record.kind === "file" ? fileTagText(record.files) : "";
  const kindIcon = media ? null : swatch ? <span className="swatch" style={{ color: record.body.trim() }} title="颜色"><Icon name="circle" size={14} fill="currentColor" strokeWidth={0} /></span>
    : record.kind === "text" ? null : record.kind === "file" ? <span className="file-tag" title={fileTag}>{fileTag}</span>
    : <span className={`kind-icon ${record.kind}`}><Icon name={record.kind} size={12} /></span>;
  const note = status || (record.expiresAt ? "敏感记录" : "");
  const pinned = record.pinnedAt !== null;
  const named = !!record.title || editing;
  const title = named && <RecordTitle record={record} editing={editing} rename={rename} search={search} />;
  const bodyMatch = search && [record.body, ...record.files.map(file => file.original)].some(value => value.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  if (editing && !media) return <article className={`record editing-record ${selected ? "selected" : ""} ${beforeSelected ? "before-selected" : ""} ${last ? "last-record" : ""}`}
    data-id={record.id} data-index={index} tabIndex={-1}>
    <RecordTitle record={record} editing rename={rename} search={search} />
  </article>;
  return <article className={`record ${selected ? "selected" : ""} ${beforeSelected ? "before-selected" : ""} ${media ? "media-record" : ""} ${named ? "named-record" : ""} ${editing ? "editing-record" : ""} ${last ? "last-record" : ""}`}
    data-id={record.id} data-index={index} tabIndex={-1}
    onClick={event => { if (!(event.target as HTMLElement).closest("button,input") && !editing) { select(event.shiftKey); event.currentTarget.focus({ preventScroll: true }); } }}
    onDoubleClick={event => { if (!(event.target as HTMLElement).closest("button,input") && !editing) apply(); }}
    onPointerOver={hover} onPointerOut={event => { if (!event.currentTarget.contains(event.relatedTarget as Node)) leave(); }}>
    {!editing && <span className="ordinal">{ordinal}</span>}
    {media ? <>{named && <span className="media-record-title">{title}</span>}<span className="preview-wrap">{hasPreview ? <Thumbnail record={record} root={root} />
      : <span className="preview-unavailable">{record.kind === "video" ? "首帧预览不可用" : "图片预览不可用"}</span>}</span>
      {!editing && bodyMatch && <span className="media-search-match" style={{ bottom: note ? 32 : 9 }}><Highlight text={text} query={search} /></span>}
      {!editing && note && <span className="media-note">{note}</span>}</>
      : <div className="record-main"><div className="record-content">{named ? <span className="record-name">{kindIcon}{title}</span> : kindIcon}<span className="record-text"><Highlight text={text} query={search} /></span>{note && <span className="record-note">{note}</span>}</div></div>}
    {!editing && <><span className="record-indicators">
      {pinned && <span className="record-indicator pin-indicator" aria-label="已置顶" title="已置顶"><Icon name="pin" size={13} /></span>}
      {record.favorite && <span className="record-indicator favorite-indicator" aria-label="已收藏" title="已收藏"><Icon name="star-fill" size={13} /></span>}
      {record.groupId !== null && <span className="record-indicator group-indicator" aria-label="已分组" title="已分组"><Icon name="group" size={13} /></span>}
    </span>
    <div className="record-actions" role="group" aria-label="记录操作">
      <button className={`record-pin ${pinned ? "on" : ""}`} data-record-pin={record.id} title={pinned ? "取消置顶记录" : "置顶记录"} aria-label={pinned ? "取消置顶记录" : "置顶记录"} aria-pressed={pinned}
        onClick={() => action("set_record_pinned", { id: record.id, pinned: !pinned })}><Icon name="pin" size={13} /></button>
      <button className={`favorite ${record.favorite ? "on" : ""}`} data-favorite={record.id} title={record.favorite ? "取消收藏" : "收藏"} aria-label={record.favorite ? "取消收藏" : "收藏"}
        onClick={() => action("set_favorite", { id: record.id, favorite: !record.favorite })}><Icon name={record.favorite ? "star-fill" : "star"} size={13} /></button>
      <button className={`move-group ${record.groupId !== null ? "on" : ""}`} type="button" data-move-group-trigger={record.id} title="移动到分组" aria-label="移动到分组" aria-haspopup="menu" aria-expanded={moveOpen}
        onClick={event => move(event.currentTarget)}><Icon name="group" size={13} /></button>
      <button className="delete" data-delete={record.id} title="删除记录" aria-label="删除记录" onClick={() => action("delete_record", { id: record.id })}><Icon name="x" size={12} /></button>
    </div></>}
  </article>;
}
