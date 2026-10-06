/** DOM access is limited to measurements, scrolling and focus; React owns the rows. */
export type RecordViewport = {
  row: (id: string) => HTMLElement | null; ensure: (id: string) => HTMLElement | null;
};
export type ListHandle = RecordViewport & {
  visibleRecords: () => HTMLElement[]; reset: () => void; scrollBackToTop: () => void; cancelScroll: () => void;
};
