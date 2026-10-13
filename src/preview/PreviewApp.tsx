import { useCallback, useEffect, useRef, useState } from "react";
import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { PreviewPayload } from "../domain/types";
import type { CarouselHandle } from "../components/MediaCarousel";
import { PreviewCard } from "./PreviewCard";
import { applyTheme } from "../services/theme";

export function PreviewApp() {
  const [payload, setPayload] = useState<PreviewPayload | null>(null);
  const current = useRef(payload), carousel = useRef<CarouselHandle | null>(null), version = useRef(0), lastWheel = useRef(0);
  current.current = payload;
  const measured = useCallback(() => {
    const request = current.current, token = version.current;
    requestAnimationFrame(() => {
      const card = document.querySelector<HTMLElement>(".card");
      if (!request || !card || version.current !== token) return;
      void emitTo("main", "preview-measured", { id: request.id, generation: request.generation, height: card.offsetHeight });
    });
  }, []);
  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const track = (dispose: UnlistenFn) => { if (disposed) dispose(); else unlisteners.push(dispose); };
    void (async () => {
      track(await listen<PreviewPayload>("preview-content", event => {
        if (event.payload.theme) applyTheme(event.payload.theme);
        version.current++; lastWheel.current = 0; current.current = event.payload; setPayload(event.payload);
      }));
      track(await listen<number | null>("preview-clear", event => {
        if (event.payload !== null && event.payload !== current.current?.generation) return;
        version.current++; current.current = null; setPayload(null);
      }));
      track(await listen("preview-ping", () => { void emitTo("main", "preview-ready"); }));
      if (!disposed) await emitTo("main", "preview-ready");
    })();
    const send = (event: string) => { if (current.current) void emitTo("main", event, current.current.id); };
    const enter = () => send("preview-enter"), leave = () => send("preview-leave"), blur = () => send("preview-blur");
    const contextmenu = (event: MouseEvent) => { if (["text", "link", "color"].includes(current.current?.kind ?? "")) event.preventDefault(); };
    const keydown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key.toLowerCase() === "a" && !event.altKey && !event.metaKey && !event.shiftKey) event.preventDefault();
      if (event.code === "Space" && current.current && ["image", "video"].includes(current.current.kind)) { event.preventDefault(); if (!event.repeat) send("preview-space"); }
    };
    const wheel = (event: WheelEvent) => {
      if (!carousel.current || !(event.target as HTMLElement).closest(".media.multi") || !event.deltaY) return;
      event.preventDefault(); const now = performance.now(); if (now - lastWheel.current < 180) return;
      lastWheel.current = now; carousel.current.step(Math.sign(event.deltaY));
    };
    document.body.addEventListener("pointerenter", enter); document.body.addEventListener("pointerleave", leave);
    window.addEventListener("blur", blur); document.addEventListener("contextmenu", contextmenu); document.addEventListener("keydown", keydown); document.addEventListener("wheel", wheel, { passive: false });
    return () => {
      disposed = true; version.current++; unlisteners.forEach(unlisten => unlisten());
      document.body.removeEventListener("pointerenter", enter); document.body.removeEventListener("pointerleave", leave);
      window.removeEventListener("blur", blur); document.removeEventListener("contextmenu", contextmenu); document.removeEventListener("keydown", keydown); document.removeEventListener("wheel", wheel);
    };
  }, []);
  return payload && <PreviewCard key={`${payload.id}:${payload.generation}`} payload={payload} measured={measured} carouselRef={carousel} />;
}
