import { Observable } from "./observable";
export class ToastController extends Observable<{ message: string; visible: boolean }> {
  private timer?: number;
  constructor() { super({ message: "", visible: false }); }
  show = (message: string) => {
    window.clearTimeout(this.timer);
    this.update({ message, visible: true });
    this.timer = window.setTimeout(() => this.update({ visible: false }), 3500);
  };
  dispose() { window.clearTimeout(this.timer); }
}
