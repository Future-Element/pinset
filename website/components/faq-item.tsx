"use client";

import { useRef } from "react";
import { useAnalytics } from "./analytics-provider";

export function FaqItem({ questionId, question, answer }: { questionId: string; question: string; answer: string }) {
  const capture = useAnalytics();
  const wasOpen = useRef(false);

  return (
    <details onToggle={(event) => {
      const open = event.currentTarget.open;
      if (open === wasOpen.current) return;
      wasOpen.current = open;
      capture("faq_toggled", { question_id: questionId });
    }}>
      <summary>{question}<span aria-hidden="true">+</span></summary>
      <p>{answer}</p>
    </details>
  );
}
