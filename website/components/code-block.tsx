import { CopyButton } from "./copy-button";

export function CodeBlock({ children, label = "Terminal" }: { children: string; label?: string }) {
  return (
    <div className="codeBlock">
      <div className="codeHeader"><span>{label}</span><CopyButton value={children} /></div>
      <pre><code>{children}</code></pre>
    </div>
  );
}
