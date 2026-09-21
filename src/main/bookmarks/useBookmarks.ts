import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Bookmark, BookmarkInput, BookmarkPosSource } from "../../types";

export const PRESET_CATEGORIES = [
  "Exobiology",
  "Scenic",
  "Mining",
  "Guardian",
  "Thargoid",
  "Station",
  "Carrier",
  "To explore",
  "Other",
];

/** Where a bookmark's position came from, completing "Position from …". */
export const POS_SOURCE_LABELS: Record<BookmarkPosSource, string> = { journal: "your journal", poi: "the POI list", edsm: "EDSM" };

/** What the editor opens with: an existing bookmark, or prefilled fields for a new one. */
export type EditorInit = { bookmark: Bookmark } | { prefill: Partial<BookmarkInput> };

export type OpenEditor = (init: EditorInit) => void;

/** The bookmark list, re-fetched whenever the backend reports a change. */
export function useBookmarks(rev: number | undefined) {
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  // Only the newest request may set the list, so a slow reply can't overwrite a fresher one.
  const latest = useRef(0);

  const refresh = useCallback(() => {
    const request = ++latest.current;
    invoke<Bookmark[]>("list_bookmarks")
      .then((list) => {
        if (request === latest.current) setBookmarks(list);
      })
      .catch((e) => console.error("list_bookmarks failed:", e));
  }, []);

  useEffect(() => {
    if (rev !== undefined) refresh();
  }, [rev, refresh]);

  // The rev bump will also trigger a refetch; refreshing here just makes the change show up without waiting for it.
  const save = useCallback(
    async (input: BookmarkInput) => {
      const saved = await invoke<Bookmark>("save_bookmark", { input });
      refresh();
      return saved;
    },
    [refresh],
  );

  const remove = useCallback(
    async (id: string) => {
      await invoke("delete_bookmark", { id });
      refresh();
    },
    [refresh],
  );

  return { bookmarks, save, remove, refresh };
}

/** Case-insensitive comparison, as used for system names and categories. */
export function sameName(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

export function bookmarksFor(bookmarks: Bookmark[], system: string): Bookmark[] {
  return bookmarks.filter((b) => sameName(b.system, system));
}

/** Trimmed, without blanks or case-insensitive duplicates; the first spelling wins. */
export function uniqueNames(names: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const c of names) {
    const name = c.trim();
    const key = name.toLowerCase();
    if (!name || seen.has(key)) continue;
    seen.add(key);
    out.push(name);
  }
  return out;
}

/** Categories in use, presets first in their usual order, then the rest alphabetically. */
export function usedCategories(bookmarks: Bookmark[]): string[] {
  const used = uniqueNames(bookmarks.flatMap((b) => b.categories));
  const rank = (c: string) => {
    const i = PRESET_CATEGORIES.findIndex((p) => p.toLowerCase() === c.toLowerCase());
    return i < 0 ? PRESET_CATEGORIES.length : i;
  };
  return used.sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
}

/** "Sol 3 a" -> "3 a". The game reports full body names; bookmarks keep the short part. */
export function shortBodyName(body: string | null, system: string): string | null {
  if (!body) return null;
  if (sameName(body, system)) return null; // the main star, i.e. the system itself
  const prefix = `${system} `;
  if (body.toLowerCase().startsWith(prefix.toLowerCase())) return body.slice(prefix.length).trim() || null;
  return body;
}
