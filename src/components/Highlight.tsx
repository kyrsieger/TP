import { highlightSearch } from "../search-highlight";

/** The shared formatter escapes content before inserting the fixed mark tags. */
export function Highlight({ text, query }: { text: string; query: string }) {
  return <span dangerouslySetInnerHTML={{ __html: highlightSearch(text, query) }} />;
}
