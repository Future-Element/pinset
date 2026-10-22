import { DocsShell } from "@/components/docs-shell";
import { commandNavigation, getCommandGroups } from "@/lib/commands";
import { guideNavigation } from "@/lib/guides";

export default function CommandsLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const groups = commandNavigation(getCommandGroups("zh-CN"));
  return <DocsShell groups={groups} guides={guideNavigation("zh-CN")} locale="zh-CN">{children}</DocsShell>;
}
