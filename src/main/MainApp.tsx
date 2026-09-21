import { useState } from "react";
import { useSnapshot } from "../useSnapshot";
import BookmarkEditor from "./bookmarks/BookmarkEditor";
import Bookmarks from "./bookmarks/Bookmarks";
import { useBookmarks, type EditorInit } from "./bookmarks/useBookmarks";
import CurrentSystem from "./CurrentSystem";
import Nearby from "./nearby/Nearby";
import OverlayControls from "./OverlayControls";
import "./main.css";
import "./bookmarks/bookmarks.css";

const TABS = [
  { id: "current", label: "Current System" },
  { id: "bookmarks", label: "Bookmarks" },
  { id: "nearby", label: "Nearby" },
] as const;

type TabId = (typeof TABS)[number]["id"];

export default function MainApp() {
  const snapshot = useSnapshot();
  const { bookmarks, save, remove, refresh } = useBookmarks(snapshot?.bookmarksRev);
  // One editor for every tab; any of them can open it prefilled.
  const [editor, setEditor] = useState<EditorInit | null>(null);
  const [tab, setTab] = useState<TabId>("current");
  // The map stays mounted once opened, so its view survives switching tabs.
  const [nearbyOpened, setNearbyOpened] = useState(false);
  const open = (id: TabId) => {
    setTab(id);
    if (id === "nearby") setNearbyOpened(true);
  };

  return (
    <div className="app">
      <header className="topbar">
        <nav className="tabs">
          {TABS.map((t) => (
            <button key={t.id} className={t.id === tab ? "tab tab--active" : "tab"} onClick={() => open(t.id)}>
              {t.label}
            </button>
          ))}
        </nav>
        {snapshot && <OverlayControls snapshot={snapshot} />}
      </header>
      <main className={tab === "nearby" ? "content content--full" : "content"}>
        {!snapshot ? (
          <p className="muted">Loading…</p>
        ) : (
          <>
            {tab === "current" && <CurrentSystem snapshot={snapshot} bookmarks={bookmarks} openEditor={setEditor} />}
            {tab === "bookmarks" && <Bookmarks snapshot={snapshot} bookmarks={bookmarks} openEditor={setEditor} refresh={refresh} />}
            {nearbyOpened && (
              <div className="tab-panel" hidden={tab !== "nearby"}>
                <Nearby snapshot={snapshot} bookmarks={bookmarks} openEditor={setEditor} />
              </div>
            )}
          </>
        )}
      </main>
      {snapshot && (
        <footer className="statusbar">
          <span>{snapshot.commander ? `CMDR ${snapshot.commander}` : "No commander yet"}</span>
          <span>{snapshot.gameFocused ? "Game focused" : "Game not focused"}</span>
          <span title={snapshot.journalDir ?? undefined}>
            {snapshot.journalDir ? "Journal found" : "Journal folder not found (set ED_JOURNAL_DIR)"}
          </span>
        </footer>
      )}
      {editor && (
        <BookmarkEditor
          init={editor}
          bookmarks={bookmarks}
          lookupError={snapshot?.bookmarksLookupError ?? null}
          onSave={save}
          onDelete={remove}
          onClose={() => setEditor(null)}
        />
      )}
    </div>
  );
}
