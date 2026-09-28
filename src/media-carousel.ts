import { bindIndicators } from "./media-indicators";
import "./media-carousel.css";

type Loaded = { image: HTMLImageElement | null };

export function createMediaCarousel(
  stage: HTMLElement,
  indicators: HTMLElement,
  count: number,
  load: (index: number) => Promise<string>,
  unavailable: string,
  indicatorAnchor: "frame" | "image" = "image",
) {
  const cache = new Map<number, Promise<Loaded>>();
  let current = 0;
  let desired = 0;
  let slide: HTMLElement | null = null;
  let running = false;
  let version = 0;
  const updateIndicators = bindIndicators(indicators, select);
  const observer = new ResizeObserver(() => {
    stage.querySelectorAll<HTMLImageElement>(".media-slide img").forEach(fitImage);
    placeIndicators();
  });
  observer.observe(stage);

  function fitImage(image: HTMLImageElement) {
    const width = stage.clientWidth;
    const height = stage.clientHeight;
    if (!width || !height) return;
    const scale = Math.min(1, width / image.naturalWidth, height / image.naturalHeight);
    image.style.width = `${image.naturalWidth * scale}px`;
    image.style.height = `${image.naturalHeight * scale}px`;
  }

  function get(index: number): Promise<Loaded> {
    let pending = cache.get(index);
    if (!pending) {
      pending = load(index).then(async src => {
        const image = new Image();
        image.src = src;
        image.draggable = false;
        image.alt = "";
        await image.decode();
        return { image };
      }).catch(() => ({ image: null }));
      cache.set(index, pending);
    }
    return pending;
  }

  function makeSlide(media: Loaded): HTMLElement {
    const item = document.createElement("div");
    item.className = "media-slide";
    if (media.image) {
      fitImage(media.image);
      item.append(media.image);
    } else {
      const message = document.createElement("p");
      message.className = "unavailable";
      message.textContent = unavailable;
      item.append(message);
    }
    return item;
  }

  function placeIndicators(item: HTMLElement | null = slide) {
    if (!item || indicatorAnchor === "frame") return;
    const stageRect = stage.getBoundingClientRect();
    const image = item.querySelector("img");
    const imageRect = image?.getBoundingClientRect();
    const inside = imageRect && imageRect.height >= indicators.offsetHeight + 16;
    indicators.style.bottom = `${inside ? Math.max(8, stageRect.bottom - imageRect.bottom + 12) : 8}px`;
    indicators.style.maxWidth = `${Math.max(1, Math.min(stageRect.width - 16, Math.max(96, (imageRect?.width ?? stageRect.width) - 16)))}px`;
  }

  function preload() {
    for (const index of cache.keys()) {
      if (Math.abs(index - current) > 1 && index !== desired) cache.delete(index);
    }
    if (current > 0) void get(current - 1);
    if (current + 1 < count) void get(current + 1);
  }

  async function start() {
    const token = version;
    const media = await get(0);
    if (token !== version) return;
    slide = makeSlide(media);
    stage.replaceChildren(slide);
    updateIndicators(current);
    placeIndicators();
    preload();
    if (desired !== current) void process();
  }

  async function process() {
    if (running || !slide) return;
    running = true;
    const token = version;
    try {
      while (desired !== current && token === version) {
        const next = desired;
        const media = await get(next);
        if (token !== version) return;
        if (next !== desired) continue;
        const outgoing = slide;
        const incoming = makeSlide(media);
        const direction = Math.sign(next - current);
        stage.append(incoming);
        placeIndicators(incoming);
        const duration = matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 240;
        if (duration) {
          const options: KeyframeAnimationOptions = { duration, easing: "cubic-bezier(.22,.61,.36,1)", fill: "both" };
          const leave = outgoing.animate([{ transform: "translateX(0)" }, { transform: `translateX(${-direction * 100}%)` }], options);
          const enter = incoming.animate([{ transform: `translateX(${direction * 100}%)` }, { transform: "translateX(0)" }], options);
          await Promise.allSettled([leave.finished, enter.finished]);
        }
        if (token !== version) return;
        incoming.style.transform = "translateX(0)";
        stage.replaceChildren(incoming);
        incoming.getAnimations().forEach(animation => animation.cancel());
        slide = incoming;
        current = next;
        updateIndicators(current);
        placeIndicators();
        preload();
      }
    } finally {
      running = false;
    }
  }

  function select(index: number) {
    if (index < 0 || index >= count || index === desired) return;
    desired = index;
    void process();
  }

  function step(direction: number) {
    select(Math.max(0, Math.min(count - 1, desired + direction)));
  }

  function destroy() {
    version++;
    observer.disconnect();
    stage.replaceChildren();
    cache.clear();
    slide = null;
  }

  return { start, select, step, destroy };
}
