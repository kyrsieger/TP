import { useLayoutEffect, useRef, useState } from "react";
import type { SettingsController } from "../state/settings";

export function CleanupConfirmation({ count, controller }: { count: number | null; controller: SettingsController }) {
  const [allRecords, setAllRecords] = useState(false);
  const ref = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => { const dialog = ref.current!; dialog.showModal(); dialog.querySelector<HTMLButtonElement>("[data-cancel]")?.focus(); return () => dialog.close(); }, []);
  return <dialog ref={ref} id="settings-cleanup-confirmation" aria-labelledby="cleanup-confirmation-title" aria-describedby="cleanup-confirmation-description"
    onCancel={event => { event.preventDefault(); controller.confirm(false); }}>
    <h2 id="cleanup-confirmation-title">{count === null ? "清空数据？" : "应用设置并清理记录？"}</h2>
    <p id="cleanup-confirmation-description">{count === null ? "清空后记录及对应的本地文件无法找回，请确认清空范围。" : <>新设置将删除 <strong>{count}</strong> 条已有普通记录及其本地文件。</>}</p>
    {count === null && <fieldset className="cleanup-scope" aria-label="清空范围">
      <label><input type="radio" name="cleanup-scope" checked={!allRecords} onChange={() => setAllRecords(false)} />普通记录</label>
      <label><input type="radio" name="cleanup-scope" checked={allRecords} onChange={() => setAllRecords(true)} />全部记录</label>
    </fieldset>}
    <p className="hint">{count === null && allRecords ? "包含收藏和所有自建分组中的记录，分组和应用设置保留。" : "收藏和自建分组中的记录会保留。"}此操作无法撤销。</p>
    <div className="confirmation-actions"><button type="button" data-cancel autoFocus onClick={() => controller.confirm(false)}>取消</button><button type="button" className="primary" data-confirm onClick={() => controller.confirm(true, allRecords)}>{count === null ? "确认清空" : "删除并应用"}</button></div>
  </dialog>;
}
