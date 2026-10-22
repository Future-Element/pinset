import { Children, isValidElement, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { docHref, headingId } from "@/lib/doc-links";
import { CopyButton } from "./copy-button";

function textContent(children: ReactNode): string {
  return Children.toArray(children).map((child) => isValidElement<{ children?: ReactNode }>(child) ? textContent(child.props.children) : String(child)).join("");
}

export function Markdown({ source, commandSections = false }: { source: string; commandSections?: boolean }) {
  const ids = new Map<string, number>();
  const idFor = (children: ReactNode) => {
    const base = headingId(textContent(children));
    const count = ids.get(base) || 0;
    ids.set(base, count + 1);
    return `${base}${count ? `-${count}` : ""}`;
  };
  return (
    <div className="markdownBody">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => <a href={docHref(href)} target={href?.startsWith("http") ? "_blank" : undefined} rel={href?.startsWith("http") ? "noreferrer" : undefined}>{children}</a>,
          h2: ({ children }) => <h2 id={idFor(children)}>{children}</h2>,
          h3: ({ children }) => <h3 id={idFor(children)}>{children}</h3>,
          h4: ({ children }) => commandSections ? <h2 id={idFor(children)}>{children}</h2> : <h4 id={idFor(children)}>{children}</h4>,
          h5: ({ children }) => commandSections ? <h3 id={idFor(children)}>{children}</h3> : <h5 id={idFor(children)}>{children}</h5>,
          table: ({ children }) => <div className="markdownTable" tabIndex={0}><table>{children}</table></div>,
          pre: ({ children }) => {
            const code = String((children as React.ReactElement<{ children?: React.ReactNode }>)?.props?.children || "").replace(/\n$/, "");
            return <div className="markdownCode"><CopyButton value={code} /><pre>{children}</pre></div>;
          },
        }}
      >{source}</ReactMarkdown>
    </div>
  );
}
