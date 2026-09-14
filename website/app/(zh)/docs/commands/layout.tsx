import { DocsShell } from "@/components/docs-shell";
import { commandNavigation, getCommandGroups } from "@/lib/commands";

export default function CommandsLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const groups = commandNavigation(getCommandGroups("zh-CN"));
  return <DocsShell groups={groups} locale="zh-CN">{children}</DocsShell>;
}
