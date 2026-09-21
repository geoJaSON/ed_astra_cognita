import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Snapshot } from "./types";

/** Latest app state, pushed from Rust on every change. */
export function useSnapshot(): Snapshot | null {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);

  useEffect(() => {
    let cancelled = false;
    const unlisten = listen<Snapshot>("snapshot", (e) => setSnapshot(e.payload));
    // An event may beat this reply; keep whichever arrived first.
    invoke<Snapshot>("get_snapshot").then((s) => {
      if (!cancelled) setSnapshot((prev) => prev ?? s);
    });
    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
    };
  }, []);

  return snapshot;
}
