export function headingId(value: string) {
  return value.toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, "").trim().replace(/\s+/g, "-");
}

export function docHref(href: string | undefined) {
  if (!href) return href;
  const match = href.match(/^(?:\.\/)?(commands|concepts|generic-tools|ci)(\.zh-CN)?\.md(?:#(.*))?$/);
  if (!match) return href;
  const [, page, chinese, fragment] = match;
  const prefix = chinese ? "" : "/en";
  if (page === "commands") return `${prefix}/docs/commands${fragment ? `/${fragment}` : ""}`;
  return `${prefix}/docs/${page}${fragment ? `#${fragment}` : ""}`;
}
