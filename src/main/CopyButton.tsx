import { useEffect, useRef, useState } from "react";

interface Props {
  text: string;
  label?: string;
  className?: string;
  title?: string;
}

/** Copies text to the clipboard and briefly says so. */
export default function CopyButton({ text, label = "Copy", className, title }: Props) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setState("copied");
    } catch {
      setState("failed");
    }
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setState("idle"), 1200);
  };

  return (
    <button className={className} title={title} onClick={copy}>
      {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : label}
    </button>
  );
}
