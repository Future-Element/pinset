import { CopyButton } from "./copy-button";

export function CodeBlock({ children, label = "Terminal", locale = "en" }: { children: string; label?: string; locale?: "en" | "zh-CN" }) {
  return (
    <div className="codeBlock">
      <div className="codeHeader"><span>{label}</span><CopyButton value={children} label={locale === "zh-CN" ? "复制" : "Copy"} copiedLabel={locale === "zh-CN" ? "已复制" : "Copied"} errorLabel={locale === "zh-CN" ? "请选中命令手动复制" : "Select and copy manually"} /></div>
      <pre><code>{children}</code></pre>
    </div>
  );
}
