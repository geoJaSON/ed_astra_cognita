import { invoke } from "@tauri-apps/api/core";
import { formatCredits } from "../highlights";

// Payouts span orders of magnitude, so the sliders move through fixed steps rather than linearly.
export const MAPPED_STEPS = [
  50_000, 100_000, 150_000, 200_000, 300_000, 500_000, 750_000, 1_000_000, 1_500_000, 2_000_000, 3_000_000, 5_000_000,
];
export const BIO_STEPS = [
  1_000_000, 5_000_000, 7_500_000, 10_000_000, 15_000_000, 20_000_000, 30_000_000, 40_000_000, 50_000_000, 75_000_000,
  100_000_000,
];

function nearestStep(steps: number[], value: number): number {
  let best = 0;
  steps.forEach((s, i) => {
    if (Math.abs(s - value) < Math.abs(steps[best] - value)) best = i;
  });
  return best;
}

interface Props {
  label: string;
  title: string;
  steps: number[];
  /** Tauri command that stores the new minimum. */
  command: string;
  value: number;
}

export default function WorthSlider({ label, title, steps, command, value }: Props) {
  return (
    <label className="worth-slider" title={title}>
      {label}
      <input
        type="range"
        min={0}
        max={steps.length - 1}
        step={1}
        value={nearestStep(steps, value)}
        onChange={(e) => invoke(command, { min: steps[Number(e.target.value)] })}
      />
      <span className="worth-slider__value">{formatCredits(value)}</span>
    </label>
  );
}
