import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type CommandCall = <T>(name: string, args?: Record<string, unknown>) => Promise<T>;
export const call: CommandCall = (name, args = {}) => invoke(name, args);
export const panelWindow = getCurrentWindow();
export const isMacOS = navigator.userAgent.includes("Macintosh");
