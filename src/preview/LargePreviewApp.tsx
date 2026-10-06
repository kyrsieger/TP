import { useCallback, useEffect, useRef, useState } from "react";
import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { LargePreviewPayload } from "../domain/types";
import { call } from "../services/desktop";
import { MediaCarousel, type CarouselHandle } from "../components/MediaCarousel";

export function LargePreviewApp() {
  const [payload, setPayload] = useState<LargePreviewPayload | null>(null), [loaded, setLoaded] = useState(false), [failed, setFailed] = useState(false);
  const image = useRef<HTMLImageElement>(null), carousel = useRef<CarouselHandle | null>(null), current = useRef(payload), version = useRef(0), lastWheel = useRef(0);
  current.current = payload;
  const close = useCallback(() => {
    if (!current.current) return;
    version.current++; current.current = null; setPayload(null); image.current?.removeAttribute("src"); void call("hide_large_preview");
  }, []);
  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const track = (dispose: UnlistenFn) => { if (disposed) dispose(); else unlisteners.push(dispose); };
    void (async () => {
      track(await listen<LargePreviewPayload>("large-preview-content", event => {
        version.current++; lastWheel.current = 0; current.current = event.payload;
        setLoaded(false); setFailed(false); setPayload(event.payload);
      }));
      track(await listen("large-preview-clear", () => { version.current++; current.current = null; setPayload(null); image.current?.removeAttribute("src"); }));
      track(await listen("large-preview-ping", () => { void emitTo("main", "large-preview-ready"); }));
      if (!disposed) await emitTo("main", "large-preview-ready");
    })();
    const keydown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key.toLowerCase() === "a" && !event.altKey && !event.metaKey && !event.shiftKey) { event.preventDefault(); return; }
      if (event.key === "Escape" || event.key === " ") { event.preventDefault(); close(); }
    };
    const wheel = (event: WheelEvent) => {
      if (!carousel.current || !event.deltaY) return;
      event.preventDefault(); const now = performance.now(); if (now - lastWheel.current < 180) return;
      lastWheel.current = now; carousel.current.step(Math.sign(event.deltaY));
    };
    document.addEventListener("keydown", keydown); document.addEventListener("wheel", wheel, { passive: false });
    return () => { disposed = true; version.current++; unlisteners.forEach(unlisten => unlisten()); document.removeEventListener("keydown", keydown); document.removeEventListener("wheel", wheel); };
  }, [close]);
  useEffect(() => {
    const element = image.current!;
    element.removeAttribute("src");
    if (!payload || payload.count > 1) return;
    let alive = true;
    const token = version.current;
    void (async () => {
      let width = 320, height = 180;
      try {
        const src = await call<string>("preview_data", { id: payload.id, role: payload.kind === "image" ? "full" : "preview", index: 0 });
        if (!alive || token !== version.current) return;
        element.src = src; await element.decode();
        if (!alive || token !== version.current) return;
        width = element.naturalWidth; height = element.naturalHeight; setLoaded(true);
      } catch { if (!alive || token !== version.current) return; setFailed(true); }
      await emitTo("main", "large-preview-loaded", { id: payload.id, fixed: false, width, height });
    })();
    return () => { alive = false; element.removeAttribute("src"); };
  }, [payload]);
  // The outer #app remains the window's drag surface, as in the original page.
  useEffect(() => {
    const root = document.getElementById("app")!;
    const drag = (event: PointerEvent) => { if (event.button === 0 && !(event.target as HTMLElement).closest("#close, .page-indicators")) void getCurrentWindow().startDragging(); };
    root.addEventListener("pointerdown", drag); return () => root.removeEventListener("pointerdown", drag);
  }, []);
  return <>
    <span id="kind">{payload?.kind === "video" ? "视频首帧" : ""}</span><button id="close" type="button" aria-label="关闭预览" onClick={close}>×</button>
    <img id="image" ref={image} alt={payload?.kind === "video" ? "视频首帧预览" : "图片预览"} draggable={false} hidden={!loaded || !payload} />
    {payload && payload.count > 1 ? <MediaCarousel key={`${payload.id}:${version.current}`} id="stage" count={payload.count} indicatorsId="indicators" carouselRef={carousel}
      load={index => call("preview_data", { id: payload.id, role: payload.kind === "image" ? "full" : "preview", index })}
      unavailable={payload.kind === "video" ? "首帧预览不可用" : "预览图不可用"}
      onReady={() => { void emitTo("main", "large-preview-loaded", { id: payload.id, fixed: true, width: 0, height: 0 }); }} /> : <div id="stage" className="media-stage" hidden />}
    <span id="error" hidden={!failed || !payload}>预览图不可用</span>{(!payload || payload.count < 2) && <div id="indicators" />}
  </>;
}
