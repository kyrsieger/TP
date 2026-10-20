import type { ThemeId } from "./themes";
import type { FontStyle } from "./fonts";
export type Kind = "text" | "link" | "image" | "file" | "video" | "color";
export type FileItem = { original: string; stored: string | null; isDirectory: boolean };
export type RecordItem = {
  id: string; kind: Kind; body: string; title: string; files: FileItem[]; favorite: boolean;
  pinnedAt: number | null; groupId: number | null; usedAt: number;
  createdAt: number | null; expiresAt: number | null;
  status: string; preview: boolean; assetPath: string | null;
};
export type Group = { id: number; name: string };
export type Settings = {
  theme?: ThemeId;
  fontFamily?: string;
  fontStyle?: FontStyle;
  maxOrdinary: number; retentionDays: number; sensitiveSeconds: number; shortcut: string; renameShortcut: string;
  positionMode: "fixed" | "cursor"; fixedPosition: "left" | "right" | "center";
  rememberPosition: boolean; panelWidth: number; panelHeight: number;
  panelX: number | null; panelY: number | null; alwaysOnTop: boolean; retainSearchInput: boolean; showTaskbarIcon: boolean;
  backgroundResident: boolean; retainTypeFilter: boolean;
  removeTrailingNewlines?: boolean;
};
export type Snapshot = { records: RecordItem[]; groups: Group[]; settings: Settings };
export type SettingsSaveResult = { status: "saved" } | { status: "confirmation_required"; count: number };
export type PanelSession = { view: string; kindFilter: string; search: string };
export type PreviewPayload = {
  removeTrailingNewlines?: boolean;
  theme?: ThemeId;
  id: string; kind: Kind; body: string; title?: string; files: { original: string }[];
  groupName: string | null; preview: boolean; status: string;
  width: number; maxHeight: number; generation: number; search: string;
};
export type LargePreviewPayload = { id: string; kind: "image" | "video"; count: number; theme?: ThemeId };
