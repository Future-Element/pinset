"use client";

import { useState } from "react";

export function CopyButton({ value, label = "Copy", copiedLabel = "Copied", errorLabel = "Select and copy manually" }: { value: string; label?: string; copiedLabel?: string; errorLabel?: string }) {
  const [copied, setCopied] = useState(false);
  const [failed, setFailed] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
      setFailed(false);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setFailed(true);
    }
  }

  return (
    <button className="copyButton" type="button" onClick={copy} aria-label={label}>
      <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 6V4a2 2 0 0 1 2-2h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-2M4 7h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2Z" /></svg>
      <span aria-live="polite">{failed ? errorLabel : copied ? copiedLabel : label}</span>
    </button>
  );
}
