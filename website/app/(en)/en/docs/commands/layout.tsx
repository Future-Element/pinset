import { DocsShell } from "@/components/docs-shell";
import { commandNavigation, getCommandGroups } from "@/lib/commands";
import { guideNavigation } from "@/lib/guides";

export default function CommandsLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const groups = commandNavigation(getCommandGroups("en"));
  return <DocsShell groups={groups} guides={guideNavigation("en")} locale="en">{children}</DocsShell>;
}
