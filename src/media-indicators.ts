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
