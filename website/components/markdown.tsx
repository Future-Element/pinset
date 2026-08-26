import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { CopyButton } from "./copy-button";

export function Markdown({ source }: { source: string }) {
  return (
    <div className="markdownBody">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => <a href={href} target={href?.startsWith("http") ? "_blank" : undefined} rel={href?.startsWith("http") ? "noreferrer" : undefined}>{children}</a>,
          pre: ({ children }) => {
            const code = String((children as React.ReactElement<{ children?: React.ReactNode }>)?.props?.children || "").replace(/\n$/, "");
            return <div className="markdownCode"><CopyButton value={code} /><pre>{children}</pre></div>;
          },
        }}
      >{source}</ReactMarkdown>
    </div>
  );
}
