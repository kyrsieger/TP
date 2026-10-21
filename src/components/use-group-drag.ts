import { useEffect, useRef, useState } from "react";
import type { RefObject, PointerEvent as ReactPointerEvent } from "react";

type Drag = { id: number; before: number | null; offset: number };
type Press = { id: number; pointer: number; capture: HTMLElement; startX: number; startY: number; startLeft: number; width: number; startScroll: number; x: number; active: boolean; timer: ReturnType<typeof setTimeout>; frame: number };

export function useGroupDrag(ref: RefObject<HTMLElement | null>, enabled: boolean, ids: number[], save: (ids: number[]) => void) {
  const [drag, setDrag] = useState<Drag | null>(null);
  const press = useRef<Press | null>(null);
  const suppressClick = useRef(false);
  const latest = useRef({ ids, save });
  latest.current = { ids, save };
  const target = useRef<number | null>(null);
  const cancel = () => {
    const current = press.current;
    if (!current) return;
    clearTimeout(current.timer);
    cancelAnimationFrame(current.frame);
    press.current = null;
    setDrag(null);
    if (current.capture.hasPointerCapture(current.pointer)) current.capture.releasePointerCapture(current.pointer);
  };
  const update = () => {
    const current = press.current, nav = ref.current;
    if (!current?.active || !nav) return;
    const buttons = [...nav.querySelectorAll<HTMLButtonElement>('[data-view^="group:"]')];
    const before = buttons.find(button => Number(button.dataset.view!.slice(6)) !== current.id &&
      current.x < button.getBoundingClientRect().left + button.offsetWidth / 2);
    target.current = before ? Number(before.dataset.view!.slice(6)) : null;
    const bounds = nav.getBoundingClientRect();
    // Keep the floating label inside the viewport so its transform cannot grow scrollWidth.
    const left = Math.max(bounds.left, Math.min(bounds.right - current.width, current.startLeft + current.x - current.startX));
    setDrag({ id: current.id, before: target.current, offset: left - current.startLeft + nav.scrollLeft - current.startScroll });
  };
  useEffect(() => {
    if (!enabled) cancel();
  }, [enabled]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (!press.current) return;
      if (event.key === "Escape") { event.preventDefault(); event.stopImmediatePropagation(); cancel(); }
      else if (press.current.active) { event.preventDefault(); event.stopImmediatePropagation(); }
    };
    const hide = () => { if (document.hidden) cancel(); };
    document.addEventListener("keydown", key, true);
    document.addEventListener("visibilitychange", hide);
    window.addEventListener("blur", cancel);
    return () => {
      cancel(); document.removeEventListener("keydown", key, true);
      document.removeEventListener("visibilitychange", hide); window.removeEventListener("blur", cancel);
    };
  }, []);
  return {
    drag,
    handlers: {
      onPointerDown(event: ReactPointerEvent<HTMLElement>) {
        suppressClick.current = false;
        if (!enabled || event.button !== 0 || !event.isPrimary || press.current) return;
        const button = (event.target as Element).closest<HTMLButtonElement>('[data-view^="group:"]');
        if (!button) return;
        const bounds = button.getBoundingClientRect();
        const current: Press = { id: Number(button.dataset.view!.slice(6)), pointer: event.pointerId, capture: button,
          startX: event.clientX, startY: event.clientY, startLeft: bounds.left, width: bounds.width, startScroll: ref.current!.scrollLeft, x: event.clientX, active: false, frame: 0,
          timer: setTimeout(() => {
            current.active = true; suppressClick.current = true;
            const tick = () => {
              if (press.current !== current) return;
              const nav = ref.current!; const bounds = nav.getBoundingClientRect();
              const speed = current.x < bounds.left + 28 ? -8 : current.x > bounds.right - 28 ? 8 : 0;
              if (speed) nav.scrollLeft += speed;
              update(); current.frame = requestAnimationFrame(tick);
            };
            tick();
          }, 200) };
        press.current = current;
        button.setPointerCapture(event.pointerId);
      },
      onPointerMove(event: ReactPointerEvent<HTMLElement>) {
        const current = press.current;
        if (!current || current.pointer !== event.pointerId) return;
        current.x = event.clientX;
        if (!current.active && Math.hypot(event.clientX - current.startX, event.clientY - current.startY) > 6) { cancel(); return; }
        if (current.active) { event.preventDefault(); update(); }
      },
      onPointerUp(event: ReactPointerEvent<HTMLElement>) {
        const current = press.current;
        if (!current || current.pointer !== event.pointerId) return;
        if (current.active) {
          current.x = event.clientX; update();
          const order = latest.current.ids.filter(id => id !== current.id);
          const index = target.current === null ? order.length : order.indexOf(target.current);
          order.splice(index < 0 ? order.length : index, 0, current.id);
          cancel(); latest.current.save(order);
        } else cancel();
      },
      onPointerCancel: cancel,
      onLostPointerCapture: cancel,
      onClickCapture(event: React.MouseEvent<HTMLElement>) {
        if (suppressClick.current && event.detail > 0) { event.preventDefault(); event.stopPropagation(); }
      },
      onDoubleClickCapture(event: React.MouseEvent<HTMLElement>) {
        if (suppressClick.current) { event.preventDefault(); event.stopPropagation(); }
      },
      onContextMenuCapture(event: React.MouseEvent<HTMLElement>) {
        if (press.current?.active) { event.preventDefault(); event.stopPropagation(); cancel(); }
      },
    },
  };
}
