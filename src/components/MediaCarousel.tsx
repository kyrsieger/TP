import { useEffect, useRef, type Ref } from "react";
import { createMediaCarousel } from "../media-carousel";
import "../media-indicators.css";

export type CarouselHandle = { step: (direction: number) => void };
/** Isolates the seek/slide animation engine in a subtree React never reconciles. */
export function MediaCarousel({ count, load, unavailable, anchor = "image", carouselRef, onReady, id, className = "media-stage", indicatorsId }: {
  count: number; load: (index: number) => Promise<string>; unavailable: string; anchor?: "frame" | "image";
  carouselRef: Ref<CarouselHandle | null>; onReady: () => void; id?: string; className?: string; indicatorsId?: string;
}) {
  const stage = useRef<HTMLDivElement>(null), indicators = useRef<HTMLDivElement>(null);
  const callbacks = useRef({ load, onReady });
  callbacks.current = { load, onReady };
  useEffect(() => {
    let alive = true;
    const carousel = createMediaCarousel(stage.current!, indicators.current!, count, index => callbacks.current.load(index), unavailable, anchor);
    if (typeof carouselRef === "function") carouselRef(carousel);
    else if (carouselRef) carouselRef.current = carousel;
    void carousel.start().then(() => { if (alive) callbacks.current.onReady(); });
    return () => {
      alive = false; carousel.destroy();
      if (typeof carouselRef === "function") carouselRef(null);
      else if (carouselRef) carouselRef.current = null;
    };
  }, [count, unavailable, anchor, carouselRef]);
  const buttons = <div className="page-indicators" ref={indicators} role="group" aria-label="预览页码">{Array.from({ length: count }, (_, index) =>
    <button key={index} className="page-indicator" type="button" data-index={index} aria-label={`查看第 ${index + 1} 项预览`} aria-pressed={index === 0} />)}</div>;
  return <><div ref={stage} id={id} className={className} />{indicatorsId ? <div id={indicatorsId}>{buttons}</div> : buttons}</>;
}
