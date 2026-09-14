import { DocsShell } from "@/components/docs-shell";
import { commandNavigation, getCommandGroups } from "@/lib/commands";

export default function CommandsLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const groups = commandNavigation(getCommandGroups("en"));
  return <DocsShell groups={groups} locale="en">{children}</DocsShell>;
}
