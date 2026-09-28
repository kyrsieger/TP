export function indicatorHtml(count: number) {
  if (count < 2) return "";
  return `<div class="page-indicators" role="group" aria-label="预览页码">${Array.from({ length: count }, (_, index) => `<button class="page-indicator" type="button" data-index="${index}" aria-label="查看第 ${index + 1} 项预览" aria-pressed="${index === 0}"></button>`).join("")}</div>`;
}

export function bindIndicators(host: HTMLElement | null, onSelect: (index: number) => void) {
  if (!host) return (_index: number) => {};
  const buttons = [...host.querySelectorAll<HTMLButtonElement>(".page-indicator")];
  host.addEventListener("click", event => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>(".page-indicator");
    if (button) onSelect(Number(button.dataset.index));
  });

  return (index: number) => {
    buttons.forEach((button, position) => {
      button.classList.toggle("active", position === index);
      button.setAttribute("aria-pressed", String(position === index));
    });
    const button = buttons[index];
    if (button.offsetLeft < host.scrollLeft) host.scrollLeft = button.offsetLeft - 8;
    if (button.offsetLeft + button.offsetWidth > host.scrollLeft + host.clientWidth) {
      host.scrollLeft = button.offsetLeft + button.offsetWidth - host.clientWidth + 8;
    }
  };
}
