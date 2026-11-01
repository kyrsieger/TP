import { useEffect, useRef, useSyncExternalStore } from "react";
import { t, getSystemLocale, subscribeLanguage, type LanguagePreference } from "../i18n";
import { Icon } from "./Icon";

/** Inline vectors keep flags consistent on Windows, where flag emoji may show letters. */
function LanguageIcon({ language }: { language: LanguagePreference }) {
  const system = useSyncExternalStore(subscribeLanguage, getSystemLocale, getSystemLocale);
  if (language === "system") language = system;
  if (language === "zh-CN") {
    const star = "0,-1 .2245,-.309 .9511,-.309 .3633,.118 .5878,.809 0,.382 -.5878,.809 -.3633,.118 -.9511,-.309 -.2245,-.309";
    return <svg className="language-icon" data-language-icon={language} viewBox="0 0 30 20" aria-hidden="true" focusable="false">
      <path fill="#de2910" d="M0 0h30v20H0z" /><g fill="#ffde00">
        <polygon points={star} transform="translate(5 5) scale(3)" />
        <polygon points={star} transform="translate(10 2) rotate(23)" /><polygon points={star} transform="translate(12 4) rotate(45)" />
        <polygon points={star} transform="translate(12 7) rotate(70)" /><polygon points={star} transform="translate(10 9) rotate(110)" />
      </g>
    </svg>;
  }
  return <svg className="language-icon" data-language-icon={language} viewBox="0 0 60 40" aria-hidden="true" focusable="false">
    <path fill="#012169" d="M0 0h60v40H0z" /><path stroke="#fff" strokeWidth="8" d="m0 0 60 40m0-40L0 40" />
    <path stroke="#c8102e" strokeWidth="3" d="m0 0 60 40m0-40L0 40" />
    <path stroke="#fff" strokeWidth="13" d="M30 0v40M0 20h60" /><path stroke="#c8102e" strokeWidth="8" d="M30 0v40M0 20h60" />
  </svg>;
}

export function LanguageSetting({ value = "system", open, toggle, choose }: {
  value?: LanguagePreference; open: boolean; toggle: (open: boolean) => void; choose: (language: LanguagePreference) => void;
}) {
  const trigger = useRef<HTMLButtonElement>(null), menu = useRef<HTMLDivElement>(null);
  const options = [["system", t("系统默认")], ["zh-CN", "简体中文"], ["en", "English"]] as const;
  const focus = (index: number) => menu.current?.querySelectorAll<HTMLButtonElement>("[role=option]")[(index + options.length) % options.length]?.focus({ preventScroll: true });
  useEffect(() => { if (open) focus(options.findIndex(([language]) => language === value)); }, [open]);
  const close = () => { toggle(false); trigger.current?.focus({ preventScroll: true }); };
  return <div className="setting-field"><span id="language-label" className="setting-label">{t("语言")}</span><div className="setting-select language-select"
    onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) toggle(false); }}>
    <button ref={trigger} id="language-trigger" type="button" aria-labelledby="language-label language-value" aria-haspopup="listbox" aria-expanded={open} aria-controls="language-menu"
      onClick={() => toggle(!open)} onKeyDown={event => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); event.stopPropagation(); toggle(true); }
      }}><LanguageIcon language={value} /><span id="language-value">{options.find(([language]) => language === value)![1]}</span><Icon name="chevron" /></button>
    <div ref={menu} id="language-menu" role="listbox" aria-labelledby="language-label" hidden={!open} onKeyDown={event => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
      else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
        event.preventDefault(); event.stopPropagation();
        const index = [...menu.current!.children].indexOf(event.target as HTMLElement);
        focus(event.key === "Home" ? 0 : event.key === "End" ? options.length - 1 : index + (event.key === "ArrowDown" ? 1 : -1));
      }
    }}>{options.map(([language, label]) => <button key={language} type="button" role="option" tabIndex={-1} data-language-option={language} aria-selected={value === language}
      onClick={() => { choose(language); close(); }}><LanguageIcon language={language} /><span>{label}</span></button>)}</div>
  </div></div>;
}
