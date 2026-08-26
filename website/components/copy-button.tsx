"use client";

import { useState } from "react";

export function CopyButton({ value, label = "Copy", copiedLabel = "Copied" }: { value: string; label?: string; copiedLabel?: string }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  return (
    <button className="copyButton" type="button" onClick={copy} aria-label={label}>
      <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 6V4a2 2 0 0 1 2-2h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-2M4 7h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2Z" /></svg>
      <span>{copied ? copiedLabel : label}</span>
    </button>
  );
}
