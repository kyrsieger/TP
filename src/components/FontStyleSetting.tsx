import { useRef } from "react";
import { fontStyleKey, type FontStyle } from "../domain/fonts";
import { Icon } from "./Icon";
import { useFontMenuWidth } from "./useFontMenuWidth";

export function FontStyleSetting({ value, styles, open, toggle, choose }: {
  value: FontStyle; styles: FontStyle[]; open: boolean; toggle: (open: boolean) => void; choose: (style: FontStyle) => void;
}) {
  const trigger = useRef<HTMLButtonElement>(null);
  const { menu, width } = useFontMenuWidth(styles.map(style => style.name), open);
  const close = () => { toggle(false); trigger.current?.focus({ preventScroll: true }); };
  const focus = (index: number) => {
    const options = menu.current!.querySelectorAll<HTMLButtonElement>("[role=option]");
    const option = options[(index + options.length) % options.length];
    option?.focus({ preventScroll: true });
    if (option) {
      const container = menu.current!;
      if (option.offsetTop < container.scrollTop) container.scrollTop = option.offsetTop;
      else if (option.offsetTop + option.offsetHeight > container.scrollTop + container.clientHeight) container.scrollTop = option.offsetTop + option.offsetHeight - container.clientHeight;
    }
  };
  return <div className="setting-field"><span id="font-style-label" className="setting-label">样式</span><div className="setting-select font-style-select">
    <button ref={trigger} id="font-style-trigger" type="button" aria-labelledby="font-style-label font-style-value" aria-haspopup="listbox" aria-expanded={open}
      aria-controls="font-style-menu" title={value.name} onClick={() => toggle(!open)} onKeyDown={event => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault(); event.stopPropagation(); toggle(true);
          requestAnimationFrame(() => focus(Math.max(0, styles.findIndex(style => fontStyleKey(style) === fontStyleKey(value)))));
        }
      }}><span id="font-style-value">{value.name}</span><Icon name="chevron" /></button>
    <div ref={menu} id="font-style-menu" style={{ width }} role="listbox" aria-labelledby="font-style-label" hidden={!open} onKeyDown={event => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
      else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
        event.preventDefault(); event.stopPropagation();
        const index = [...menu.current!.children].indexOf(event.target as HTMLElement);
        focus(event.key === "Home" ? 0 : event.key === "End" ? styles.length - 1 : index + (event.key === "ArrowDown" ? 1 : -1));
      }
    }}>{styles.map(style => <button key={fontStyleKey(style)} type="button" role="option" title={style.name} aria-selected={fontStyleKey(style) === fontStyleKey(value)}
      onClick={() => { choose(style); close(); }}>{style.name}</button>)}</div>
  </div></div>;
}
