import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { call } from "../services/desktop";
import { Icon } from "./Icon";
import { defaultFontStyle, fontStyleKey, regularFontStyle, resolveFontSelection, type FontFamily, type FontStyle } from "../domain/fonts";
import { FontStyleSetting } from "./FontStyleSetting";
import { buildFontSearchIndex, searchFontFamilies } from "../domain/font-search";
import { useFontMenuWidth } from "./useFontMenuWidth";

export function FontSetting({ value, styleValue, open, toggle, choose, styleOpen, toggleStyle }: {
  value: string; styleValue: FontStyle; open: boolean; toggle: (open: boolean) => void; choose: (family: string, style: FontStyle) => void;
  styleOpen: boolean; toggleStyle: (open: boolean) => void;
}) {
  const [fonts, setFonts] = useState<FontFamily[] | null>(null), [error, setError] = useState(false);
  const migrations = useRef(new Set<string>());
  const [query, setQuery] = useState(""), [active, setActive] = useState(value), [request, setRequest] = useState(0);
  const trigger = useRef<HTMLButtonElement>(null), search = useRef<HTMLInputElement>(null), list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open && fonts !== null) return;
    let alive = true;
    setError(false);
    void call<FontFamily[]>("get_system_fonts").then(groups => {
      if (alive) setFonts([...groups].sort((a, b) => a.name.localeCompare(b.name, "zh-CN")));
    }).catch(() => { if (alive) setError(true); });
    return () => { alive = false; };
  }, [request, open]);
  useEffect(() => {
    if (open) { setQuery(""); setActive(value); search.current?.focus({ preventScroll: true }); }
  }, [open]);
  const current = resolveFontSelection(fonts ?? [], value, styleValue);
  const currentGroup = fonts?.find(font => font.name === current.family);
  const styleKey = fontStyleKey(styleValue);
  useEffect(() => {
    const key = `${value}:${styleKey}`;
    if (value && fonts && (current.family !== value || fontStyleKey(current.style) !== styleKey) && !migrations.current.has(key)) {
      migrations.current.add(key); choose(current.family, current.style);
    }
  }, [fonts, value, styleKey]);
  const missingFamily = currentGroup ? "" : current.family;
  const searchIndex = useMemo(() => buildFontSearchIndex(fonts ?? [], missingFamily), [fonts, missingFamily]);
  const options = searchFontFamilies(searchIndex, query);
  const { menu, width } = useFontMenuWidth(searchFontFamilies(searchIndex, "").map(name => name || "系统默认"), open, true);
  const activeIndex = options.includes(active) ? options.indexOf(active) : options.length ? 0 : -1;
  useLayoutEffect(() => {
    if (!open || activeIndex < 0) return;
    const container = list.current, option = container?.children[activeIndex] as HTMLElement | undefined;
    if (!container || !option) return;
    if (option.offsetTop < container.scrollTop) container.scrollTop = option.offsetTop;
    else if (option.offsetTop + option.offsetHeight > container.scrollTop + container.clientHeight)
      container.scrollTop = option.offsetTop + option.offsetHeight - container.clientHeight;
  }, [open, activeIndex, fonts, query]);
  const close = () => { toggle(false); trigger.current?.focus({ preventScroll: true }); };
  const select = (family: string) => {
    const group = fonts?.find(font => font.name === family);
    choose(family, group ? defaultFontStyle(group) : regularFontStyle); toggleStyle(false); close();
  };
  return <><div className="setting-field"><span id="font-label" className="setting-label">字体</span><div className="setting-select font-select">
    <button ref={trigger} id="font-trigger" type="button" aria-labelledby="font-label font-value" aria-haspopup="listbox" aria-expanded={open} aria-controls="font-list"
      title={current.family || "系统默认"} onClick={() => toggle(!open)} onKeyDown={event => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); event.stopPropagation(); toggle(true); }
      }}><span id="font-value">{current.family || "系统默认"}</span><Icon name="chevron" /></button>
    <div ref={menu} id="font-menu" style={{ width }} hidden={!open} onKeyDown={event => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
      else if (event.key === "Tab") toggle(false);
    }}>
      <input ref={search} id="font-search" type="search" role="combobox" aria-label="搜索字体" placeholder="字体名、拼音或首字母" autoComplete="off"
        aria-expanded={open} aria-controls="font-list" aria-autocomplete="list" aria-activedescendant={activeIndex >= 0 ? `font-option-${activeIndex}` : undefined}
        value={query} onChange={event => {
          const next = event.target.value; setQuery(next);
          setActive(searchFontFamilies(searchIndex, next)[0] ?? "");
        }} onKeyDown={event => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault(); event.stopPropagation();
            if (options.length) setActive(options[activeIndex < 0 ? (event.key === "ArrowDown" ? 0 : options.length - 1)
              : (activeIndex + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length]);
          } else if (event.key === "Enter") {
            event.preventDefault(); event.stopPropagation(); if (activeIndex >= 0) select(options[activeIndex]);
          }
        }} />
      <div ref={list} id="font-list" role="listbox" aria-labelledby="font-label">{options.map((family, index) => <button key={family} id={`font-option-${index}`} type="button" role="option" tabIndex={-1}
        aria-selected={current.family === family} className={activeIndex === index ? "font-active" : ""} title={family || "系统默认"}
        onPointerEnter={() => setActive(family)} onMouseDown={event => event.preventDefault()} onClick={() => select(family)}>{family || "系统默认"}</button>)}</div>
      {error ? <button type="button" className="font-status" onClick={() => setRequest(previous => previous + 1)}>读取字体失败，点击重试</button>
        : fonts === null ? <p className="font-status" role="status">正在读取系统字体…</p>
        : options.length === 0 && <p className="font-status" role="status">没有匹配的字体</p>}
    </div>
  </div></div>{current.family && currentGroup && currentGroup.styles.length > 1 && <FontStyleSetting
    value={current.style} styles={currentGroup.styles} open={styleOpen} toggle={toggleStyle} choose={style => choose(current.family, style)} />}</>;
}
