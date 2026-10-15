import { useLayoutEffect, useRef } from "react";
import type { SettingsController } from "../state/settings";

export function CleanupConfirmation({ count, controller }: { count: number | null; controller: SettingsController }) {
  const ref = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => { const dialog = ref.current!; dialog.showModal(); return () => dialog.close(); }, []);
  return <dialog ref={ref} id="settings-cleanup-confirmation" aria-labelledby="cleanup-confirmation-title" aria-describedby="cleanup-confirmation-description"
    onCancel={event => { event.preventDefault(); controller.confirm(false); }}>
    <h2 id="cleanup-confirmation-title">{count === null ? "清理缓存？" : "应用设置并清理记录？"}</h2>
    <p id="cleanup-confirmation-description">{count === null ? "删除所有普通记录、对应的本地文件及孤立附件。" : <>新设置将删除 <strong>{count}</strong> 条已有普通记录及其本地文件。</>}</p>
    <p className="hint">收藏和自建分组中的记录会保留。此操作无法撤销。</p>
    <div className="confirmation-actions"><button type="button" data-cancel autoFocus onClick={() => controller.confirm(false)}>取消</button><button type="button" className="primary" data-confirm onClick={() => controller.confirm(true)}>{count === null ? "清理缓存" : "删除并应用"}</button></div>
  </dialog>;
}
