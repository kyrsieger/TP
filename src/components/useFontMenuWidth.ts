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
        context.font = `${style.fontStyle} ${style.fontWeight} 12px ${style.fontFamily}`;
        const normal = context.measureText(label).width;
        context.font = `${style.fontStyle} 600 12px ${style.fontFamily}`;
        return Math.max(normal, context.measureText(label).width);
      }));
      const available = row.clientWidth - parseFloat(rowStyle.paddingLeft) - parseFloat(rowStyle.paddingRight);
      setWidth(Math.min(available, Math.max(searchable ? 180 : 0, Math.ceil(longest) + 40)));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(row);
    // Font selection also changes the font used by the controls themselves.
    observer.observe(element.parentElement!);
    void document.fonts.ready.then(measure);
    document.fonts.addEventListener("loadingdone", measure);
    return () => { alive = false; observer.disconnect(); document.fonts.removeEventListener("loadingdone", measure); };
  }, [signature, open, searchable]);
  return { menu, width };
}
