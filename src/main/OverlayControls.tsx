import { invoke } from "@tauri-apps/api/core";
import type { Snapshot } from "../types";

export default function OverlayControls({ snapshot }: { snapshot: Snapshot }) {
  const { overlay } = snapshot;
  return (
    <div className="overlay-controls">
      <label title="Toggle anywhere with Ctrl+Alt+O">
        <input
          type="checkbox"
          checked={overlay.enabled}
          onChange={(e) => invoke("set_overlay_enabled", { enabled: e.target.checked })}
        />
        Overlay <kbd>Ctrl+Alt+O</kbd>
      </label>
      <label title="Show the overlay even when the game isn't focused">
        <input
          type="checkbox"
          checked={overlay.forceShow}
          onChange={(e) => invoke("set_overlay_force_show", { force: e.target.checked })}
        />
        Always show
      </label>
      <button onClick={() => invoke("set_overlay_unlocked", { unlocked: !overlay.unlocked })}>
        {overlay.unlocked ? "Lock overlay" : "Move overlay"}
      </button>
    </div>
  );
}
