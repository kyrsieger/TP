import { useLayoutEffect, useRef, useState } from "react";

// Measure the complete catalog, so filtering does not make the menu jump in width.
export function useFontMenuWidth(labels: string[], open: boolean, searchable = false) {
  const menu = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState<number>();
  const signature = JSON.stringify(labels);
  useLayoutEffect(() => {
    if (!open || !menu.current) return;
    const element = menu.current;
    const row = element.closest<HTMLElement>(".setting-field")!;
    let alive = true;
    const measure = () => {
      if (!alive) return;
      const style = getComputedStyle(element), rowStyle = getComputedStyle(row);
      const context = document.createElement("canvas").getContext("2d");
      if (!context) return;
      // Selected options can be semibold; reserve enough space for them too.
      const longest = Math.max(0, ...JSON.parse(signature).map((label: string) => {
        context.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
        const normal = context.measureText(label).width;
        context.font = `${style.fontStyle} 600 ${style.fontSize} ${style.fontFamily}`;
        return Math.max(normal, context.measureText(label).width);
      }));
      const available = row.clientWidth - parseFloat(rowStyle.paddingLeft) - parseFloat(rowStyle.paddingRight);
      const nextWidth = Math.min(available, Math.max(searchable ? 180 : 0, Math.ceil(longest) + 40));
      setWidth(nextWidth);
      // Keep menus visible after scrolling the appearance card or changing text size.
      const anchor = element.parentElement!.getBoundingClientRect();
      const settings = row.closest("#settings")!.getBoundingClientRect();
      const topEdge = settings.top + 8, bottomEdge = Math.min(settings.bottom, innerHeight) - 8;
      const below = bottomEdge - anchor.bottom - 5, above = anchor.top - 5 - topEdge;
      const upwards = below < 180 && above > below;
      const room = Math.max(60, upwards ? above : below);
      const list = searchable ? element.querySelector<HTMLElement>("#font-list") : null;
      if (list) list.style.maxHeight = `${Math.max(30, Math.min(240, Math.floor(room - 53)))}px`;
      else element.style.maxHeight = `${Math.min(240, room)}px`;
      element.style.position = "fixed";
      element.style.width = `${nextWidth}px`;
      element.style.right = "auto";
      element.style.left = `${Math.max(8, Math.min(anchor.right - nextWidth, innerWidth - nextWidth - 8))}px`;
      element.style.top = `${Math.max(topEdge, Math.min(upwards ? anchor.top - 5 - element.offsetHeight : anchor.bottom + 5, bottomEdge - element.offsetHeight))}px`;

    };
    measure();
    window.addEventListener("scroll", measure, true);
    window.addEventListener("resize", measure);
    const observer = new ResizeObserver(measure);
    observer.observe(row);
    // Font selection also changes the font used by the controls themselves.
    observer.observe(element.parentElement!);
    void document.fonts.ready.then(measure);
    document.fonts.addEventListener("loadingdone", measure);
    return () => { alive = false; window.removeEventListener("scroll", measure, true); window.removeEventListener("resize", measure); observer.disconnect(); document.fonts.removeEventListener("loadingdone", measure); };
  }, [signature, open, searchable]);
  return { menu, width };
}
