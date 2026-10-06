import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PreviewPayload } from "../domain/types";
import { labels } from "../domain/records";
import { call } from "../services/desktop";
import { Highlight } from "../components/Highlight";
import { MediaCarousel, type CarouselHandle } from "../components/MediaCarousel";

function FileText({ id, index, measured }: { id: string; index: number; measured: () => void }) {
  const [result, setResult] = useState<{ content: string | null; error: string } | null>(null);
  useEffect(() => {
    let alive = true;
    void call<string | null>("file_text_preview", { id, index }).then(content => { if (alive) setResult({ content, error: "" }); })
      .catch(error => { if (alive) setResult({ content: null, error: String(error) }); });
    return () => { alive = false; };
  }, [id, index]);
  useLayoutEffect(() => { if (result) measured(); }, [result]);
  if (!result || (result.content === null && !result.error)) return null;
  return result.content === null ? <p className="file-text-error">文本预览不可用：{result.error}</p> : <pre className="file-text-content">{result.content}</pre>;
}
function SingleMedia({ payload, measured }: { payload: PreviewPayload; measured: () => void }) {
  const ref = useRef<HTMLImageElement>(null);
  const [loaded, setLoaded] = useState(false), [failed, setFailed] = useState(false);
  useEffect(() => {
    let alive = true;
    const image = ref.current!;
    void (async () => {
      try {
        const src = await call<string>("preview_data", { id: payload.id, role: "preview", index: 0 });
        if (!alive) return;
        image.src = src; await image.decode();
        if (alive) setLoaded(true);
      } catch { if (alive) setFailed(true); }
    })();
    return () => { alive = false; image.removeAttribute("src"); };
  }, [payload.id]);
  useLayoutEffect(() => { if (loaded || failed) measured(); }, [loaded, failed]);
  return <div className="media"><img id="large-preview" ref={ref} alt={`${labels[payload.kind]}预览`} hidden={!loaded} />
    <p id="media-error" className="unavailable" hidden={!failed}>{failed ? payload.kind === "video" ? "首帧预览不可用" : "预览图不可用" : ""}</p></div>;
}
export function PreviewCard({ payload, measured, carouselRef }: { payload: PreviewPayload; measured: () => void; carouselRef: React.Ref<CarouselHandle | null> }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const area = ref.current!.querySelector<HTMLElement>(".text-content"), match = area?.querySelector<HTMLElement>(".search-hit");
    if (area && match) area.scrollTop = match.getBoundingClientRect().top - area.getBoundingClientRect().top - area.clientHeight / 2;
    if (payload.kind !== "image" && payload.kind !== "video") measured();
  }, [payload, measured]);
  const color = /^#(?:[\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8})$/i.test(payload.body.trim()) ? payload.body.trim() : "";
  const status = payload.status === "missing" ? "文件已不可用" : payload.status === "copy_failed" ? "副本保存失败" : "";
  return <div ref={ref} className={`card${payload.kind === "text" ? " text-card" : ""}`} style={{ width: payload.width, maxHeight: payload.kind === "text" ? payload.maxHeight : undefined }}>
    {(payload.kind === "text" || payload.kind === "link") && <pre className="text-content"><Highlight text={payload.body} query={payload.search} /></pre>}
    {payload.kind === "color" && <div className="color-content"><span className="color-disc" style={{ background: color }} /><span><Highlight text={payload.body} query={payload.search} /></span></div>}
    {(payload.kind === "image" || payload.kind === "video") && (payload.files.length > 1
      ? <div className="media multi"><MediaCarousel count={payload.files.length} load={index => call("preview_data", { id: payload.id, role: "preview", index })}
        unavailable={payload.kind === "video" ? "首帧预览不可用" : "预览图不可用"} anchor="frame" carouselRef={carouselRef} onReady={measured} /></div>
      : <SingleMedia payload={payload} measured={measured} />)}
    {payload.groupName !== null && <dl className="details"><div><dt>分组</dt><dd>{payload.groupName}</dd></div></dl>}
    {payload.files.length > 0 && <ul className={`files${payload.files.length === 1 ? " single-file" : ""}`}>{payload.files.map((file, index) => {
      const name = file.original.split(/[\\/]/).pop() || file.original;
      return <li key={index} className="file" data-index={index}><div className="path"><Highlight text={file.original.slice(0, file.original.length - name.length)} query={payload.search} /><strong><Highlight text={name} query={payload.search} /></strong></div>
        {payload.kind === "file" && payload.files.length === 1 && <FileText id={payload.id} index={index} measured={measured} />}</li>;
    })}</ul>}
    {status && <div className="status-note">{status}</div>}
  </div>;
}
