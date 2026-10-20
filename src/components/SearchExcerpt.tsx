import { useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { prepareSearchExcerpt } from "../search-excerpt";
import { Highlight } from "./Highlight";
import { onFontChange } from "../services/fonts";

let context: CanvasRenderingContext2D | null | undefined;

export function SearchExcerpt({ text, query, className, style }: {
  text: string; query: string; className: string; style?: CSSProperties;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const singleLine = useMemo(() => text.replace(/\r\n|[\r\n\t]/g, " "), [text]);
  const fit = useMemo(() => query ? prepareSearchExcerpt(singleLine, query) : null, [singleLine, query]);
  const [result, setResult] = useState({ fit, text: query ? "" : text });
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    if (!fit) { setResult({ fit, text }); return; }
    const update = () => {
      context ??= document.createElement("canvas").getContext("2d");
      if (!context) { setResult({ fit, text }); return; }
      const computed = getComputedStyle(element);
      context.font = computed.font;
      const width = element.clientWidth - parseFloat(computed.paddingLeft) - parseFloat(computed.paddingRight);
      const excerpt = fit(width, value => context!.measureText(value).width);
      setResult(previous => previous.fit === fit && previous.text === excerpt ? previous : { fit, text: excerpt });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    const unlisten = onFontChange(update);
    return () => { observer.disconnect(); unlisten(); };
  }, [text, fit]);
  const excerpt = result.fit === fit ? result.text : fit ? "" : text;
  return <span ref={ref} className={`${className}${query ? " search-excerpt" : ""}`} style={{ ...style, whiteSpace: query ? "pre" : undefined }}><Highlight text={excerpt} query={query} /></span>;
}
