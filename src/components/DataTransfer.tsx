import { useLayoutEffect, useRef } from "react";
import type { TransferController } from "../state/transfer";
import { useObservable } from "./use-observable";

export function DataTransfer({ controller }: { controller: TransferController }) {
  const state = useObservable(controller);
  return <section className="settings-card data-transfer" aria-labelledby="data-transfer-title">
    <div className="transfer-copy"><h4 id="data-transfer-title">导出与导入</h4>
    <p className="hint">导出包可直接浏览，也可重新导入 TP。不包含设置和敏感标记记录，ZIP 包未加密。</p></div>
    <div className="transfer-actions">
      <button id="export-data" type="button" disabled={state.busy} onClick={() => controller.requestExport()}>导出</button>
      <button id="import-data" type="button" disabled={state.busy} onClick={() => { void controller.inspect(); }}>导入</button>
    </div>
  </section>;
}

export function TransferDialog({ controller }: { controller: TransferController }) {
  const state = useObservable(controller), ref = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => { const dialog = ref.current!; dialog.showModal(); return () => dialog.close(); }, []);
  const restore = state.mode === "import", preview = state.preview;
  const title = state.busy ? restore ? preview ? "正在导入…" : "正在读取导出包…" : "正在导出…"
    : restore ? "导入？" : "导出";
  return <dialog ref={ref} id="data-transfer-dialog" data-restore={restore} aria-labelledby="transfer-dialog-title" aria-describedby="transfer-dialog-description"
    aria-busy={state.busy} onCancel={event => { event.preventDefault(); controller.cancel(); }}>
    <h2 id="transfer-dialog-title">{title}</h2>
    <p id="transfer-dialog-description">{restore ? "将替换全部现有记录和自建分组。此操作无法撤销，应用设置沿用本机配置。"
      : "导出全部历史记录、整理信息和 TP 持有的附件。解压后可浏览内容，也可使用同一个 ZIP 重新导入 TP。"}</p>
    {!restore && <p>ZIP 包未加密，不包含应用设置和当前带敏感到期标记的记录。路径引用只保留原路径，不额外读取用户原文件。</p>}
    {preview && <>
      <p>导出包包含 <strong>{preview.records}</strong> 条记录、{preview.groups} 个自建分组。</p>
      {preview.nextMaxOrdinary !== preview.currentMaxOrdinary && <p>为保留导入数据，普通记录数量上限将从 {preview.currentMaxOrdinary} 自动调整为 <strong>{preview.nextMaxOrdinary}</strong>。</p>}
      <p>按本机保留规则，<strong>{preview.cleanupCount}</strong> 条普通记录会因时间过期立即清理。</p>
      {preview.cleanupCount > 0 && <label className="keep-expired"><input id="keep-expired" type="checkbox" disabled={state.busy} checked={state.keepExpired} onChange={event => controller.update({ keepExpired: event.target.checked })} />保留这些过期记录，将其最后使用时间改为当前时间，创建时间不变。</label>}
      {preview.warnings.length > 0 && <details><summary>文件可用性说明（{preview.warnings.length} 项）</summary><ul>{preview.warnings.map((warning, index) => <li key={index}>{warning}</li>)}</ul></details>}
      <p className="hint">文件缺失时沿用“文件已不可用”的标记，已有预览可能仍可显示。</p>
    </>}
    {state.error && <p className="transfer-error" role="alert">{state.error}</p>}
    {state.busy ? <p className="transfer-working" role="status">正在处理，请稍候。大附件可能需要较长时间。</p>
      : <div className="confirmation-actions"><button type="button" autoFocus onClick={() => controller.cancel()}>取消</button><button type="button" className="primary" disabled={restore && !preview} onClick={() => { void controller.confirm(); }}>{restore ? "确认导入" : "选择保存位置"}</button></div>}
  </dialog>;
}
