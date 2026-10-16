import {
  ArrowUpToLine, Check, ChevronDown, Circle, Clipboard, File, Folder, Funnel, Image,
  Link, Minus, Palette, Pin, Play, Plus, Search, Settings, Square, Star, Type, Video, X,
  type LucideProps,
} from "lucide-react";

const icons = {
  "arrow-up-to-line": ArrowUpToLine,
  check: Check,
  chevron: ChevronDown,
  circle: Circle,
  clipboard: Clipboard,
  color: Palette,
  file: File,
  filter: Funnel,
  gear: Settings,
  group: Folder,
  image: Image,
  link: Link,
  minus: Minus,
  pin: Pin,
  play: Play,
  plus: Plus,
  search: Search,
  square: Square,
  star: Star,
  "star-fill": Star,
  text: Type,
  video: Video,
  x: X,
} as const;

type IconProps = Omit<LucideProps, "name"> & { name: keyof typeof icons };

export function Icon({ name, size = 14, ...props }: IconProps) {
  const Glyph = icons[name];
  return <Glyph size={size} strokeWidth={1.8} fill={name === "star-fill" ? "currentColor" : "none"}
    aria-hidden="true" focusable="false" {...props} />;
}
