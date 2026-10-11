import type { PanelController } from "../state/panel";
import type { SettingsController } from "../state/settings";
import type { MenuController } from "../state/menus";
import type { PreviewController } from "../services/previews";
import type { ListHandle } from "../services/viewport";
export type KeyboardContext = {
  history: PanelController; previews: PreviewController; menus: MenuController; settings: SettingsController;
  list: ListHandle; clearSearch: (refocus?: boolean) => void; cycleGroup: (direction: number) => void;
  closeSettings: () => void; moveSelection: (direction: number, extend?: boolean) => void; isMacOS: boolean;
  startRecordRename: (id: string) => void;
};
