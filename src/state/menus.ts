import type { RecordItem } from "../domain/types";
import { Observable } from "./observable";

export type Point = { x: number; y: number };
type MoveMenu = { ids: string[]; triggerId: string; anchor: { top: number; bottom: number; right: number } };
export class MenuController extends Observable<{
  group: ({ id: number } & Point) | null; record: ({ id: string } & Point) | null; move: MoveMenu | null;
  filter: boolean; retention: boolean; fixedPosition: boolean;
}> {
  constructor() { super({ group: null, record: null, move: null, filter: false, retention: false, fixedPosition: false }); }
  closePopups() { this.update({ group: null, record: null, move: null }); }
  reset() { this.closePopups(); this.update({ filter: false, retention: false, fixedPosition: false }); }
  openMove(trigger: HTMLElement, anchor = trigger, records: RecordItem[]) {
    const rect = anchor.getBoundingClientRect();
    this.update({ move: { ids: records.map(record => record.id), triggerId: trigger.dataset.moveGroupTrigger!,
      anchor: { top: rect.top, bottom: rect.bottom, right: rect.right } } });
  }
}
