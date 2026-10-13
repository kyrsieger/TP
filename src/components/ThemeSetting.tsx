import { useRef } from "react";
import { getTheme, sortedThemes, themeSwatches, type ThemeId } from "../domain/themes";
import { Icon } from "./Icon";

export function ThemeSetting({ value, open, toggle, choose }: {
  value: ThemeId | undefined; open: boolean; toggle: (open: boolean) => void; choose: (theme: ThemeId) => void;
}) {
  const current = getTheme(value), trigger = useRef<HTMLButtonElement>(null), menu = useRef<HTMLDivElement>(null);
  const focusOption = (option: HTMLButtonElement | undefined) => {
    if (!option) return;
    option.focus({ preventScroll: true });
    const container = menu.current!;
    if (option.offsetTop < container.scrollTop) container.scrollTop = option.offsetTop;
    else if (option.offsetTop + option.offsetHeight > container.scrollTop + container.clientHeight) container.scrollTop = option.offsetTop + option.offsetHeight - container.clientHeight;
  };
  const move = (direction: number, target?: EventTarget) => {
    const options = [...menu.current!.querySelectorAll<HTMLButtonElement>("[role=option]")];
    const index = target ? options.indexOf(target as HTMLButtonElement) : sortedThemes.findIndex(theme => theme.id === current.id);
    focusOption(options[(index + direction + options.length) % options.length]);
  };
  const close = () => { toggle(false); trigger.current?.focus({ preventScroll: true }); };
  return <div className="setting-field"><span id="theme-label" className="setting-label">主题配色</span><div className="setting-select theme-select">
    <button ref={trigger} id="theme-trigger" type="button" aria-labelledby="theme-label theme-value" aria-haspopup="listbox" aria-expanded={open} aria-controls="theme-menu"
      onClick={() => toggle(!open)} onKeyDown={event => {
        if (event.key === "Escape" && open) { event.preventDefault(); event.stopPropagation(); close(); }
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault(); event.stopPropagation(); toggle(true);
          requestAnimationFrame(() => move(event.key === "ArrowDown" ? 1 : -1));
        }
      }}><span id="theme-value">{current.name}</span><Icon name="chevron" /></button>
    <div ref={menu} id="theme-menu" role="listbox" aria-labelledby="theme-label" hidden={!open} onKeyDown={event => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
      else if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); event.stopPropagation(); move(event.key === "ArrowDown" ? 1 : -1, event.target); }
      else if (event.key === "Home" || event.key === "End") {
        event.preventDefault(); event.stopPropagation(); const options = menu.current!.querySelectorAll<HTMLButtonElement>("[role=option]");
        focusOption(options[event.key === "Home" ? 0 : options.length - 1]);
      } else if (event.key === "Enter" || event.key === " ") { event.preventDefault(); event.stopPropagation(); close(); }
    }}>{sortedThemes.map(theme => <button key={theme.id} type="button" role="option" data-theme-option={theme.id} aria-selected={current.id === theme.id}
      className={theme.dark ? "dark-theme" : "light-theme"} onPointerEnter={() => choose(theme.id)} onFocus={() => choose(theme.id)}
      onClick={() => { choose(theme.id); close(); }}>
      <span className="theme-name">{theme.name}</span><span className="theme-strip" aria-hidden="true">{themeSwatches(theme).map((color, index) => <span key={index} style={{ backgroundColor: color }} />)}</span>
    </button>)}</div>
  </div><p className="hint">悬停到主题上即可切换，自动记住最后选择。</p></div>;
}
