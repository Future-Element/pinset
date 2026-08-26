"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import type { CommandGroup } from "@/lib/commands";
import type { Locale } from "@/lib/site";
import { localePrefix, siteConfig } from "@/lib/site";
import { Brand } from "./brand";

export function DocsShell({ children, groups, locale }: { children: React.ReactNode; groups: CommandGroup[]; locale: Locale }) {
  const pathname = usePathname();
  const prefix = localePrefix(locale);
  const [menuOpen, setMenuOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const zh = locale === "zh-CN";
  const otherLocaleHref = locale === "en" ? pathname.replace(/^\/en/, "") || "/" : `/en${pathname}`;

  const results = useMemo(() => {
    const value = query.trim().toLowerCase();
    const commands = groups.flatMap((group) => group.commands);
    if (!value) return commands.slice(0, 8);
    return commands.filter((command) => `${command.title} ${command.description}`.toLowerCase().includes(value)).slice(0, 12);
  }, [groups, query]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSearchOpen(true);
      }
      if (event.key === "Escape") setSearchOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (searchOpen) window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [searchOpen]);

  return (
    <div className="siteFrame" lang={locale}>
      <header className="topbar">
        <Brand href={prefix || "/"} />
        <nav className="topnav" aria-label={zh ? "主导航" : "Primary navigation"}>
          <Link className={!pathname.includes("/commands") ? "active" : ""} href={prefix || "/"}>{zh ? "文档" : "Documentation"}</Link>
          <Link className={pathname.includes("/commands") ? "active" : ""} href={`${prefix}/docs/commands`}>{zh ? "命令" : "Commands"}</Link>
        </nav>
        <div className="topActions">
          <button className="searchButton" type="button" onClick={() => setSearchOpen(true)} aria-label={zh ? "搜索命令" : "Search commands"}>
            <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m14.3 14.3 3.2 3.2m-1.7-8.2a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0Z" /></svg>
            <span>{zh ? "搜索命令" : "Search commands"}</span><kbd>⌘K</kbd>
          </button>
          <Link className="languageLink" href={otherLocaleHref} hrefLang={locale === "en" ? "zh-CN" : "en"}>{locale === "en" ? "中文" : "EN"}</Link>
          <a className="iconLink" href={siteConfig.repository} target="_blank" rel="noreferrer" aria-label="GitHub">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 2a10 10 0 0 0-3.16 19.49c.5.09.68-.22.68-.48v-1.87c-2.78.6-3.37-1.18-3.37-1.18-.45-1.16-1.11-1.47-1.11-1.47-.9-.62.07-.61.07-.61 1 .07 1.53 1.03 1.53 1.03.9 1.52 2.34 1.08 2.91.83.09-.64.35-1.08.63-1.33-2.22-.25-4.55-1.11-4.55-4.94 0-1.09.39-1.98 1.03-2.68-.1-.25-.45-1.27.1-2.64 0 0 .84-.27 2.75 1.02A9.58 9.58 0 0 1 12 6.84c.85 0 1.71.11 2.51.34 1.91-1.3 2.75-1.03 2.75-1.03.55 1.37.2 2.39.1 2.64.64.7 1.03 1.59 1.03 2.68 0 3.84-2.34 4.68-4.57 4.93.36.31.68.92.68 1.86v2.75c0 .27.18.58.69.48A10 10 0 0 0 12 2Z" /></svg>
          </a>
          <button className="menuButton" type="button" onClick={() => setMenuOpen(!menuOpen)} aria-expanded={menuOpen} aria-label={zh ? "打开导航" : "Open navigation"}><span /><span /><span /></button>
        </div>
      </header>

      <aside className={`sidebar ${menuOpen ? "open" : ""}`}>
        <div className="sidebarScroll">
          <div className="navGroup">
            <div className="navHeading">{zh ? "文档" : "Documentation"}</div>
            <Link className={pathname === prefix || pathname === `${prefix}/` ? "navLink active" : "navLink"} href={prefix || "/"} onClick={() => setMenuOpen(false)}>{zh ? "介绍" : "Introduction"}</Link>
            <Link className={pathname === `${prefix}/docs/commands` ? "navLink active" : "navLink"} href={`${prefix}/docs/commands`} onClick={() => setMenuOpen(false)}>{zh ? "命令索引" : "Command index"}</Link>
          </div>
          {groups.map((group) => (
            <div className="navGroup" key={group.title}>
              <div className="navHeading">{group.title}</div>
              {group.commands.map((command) => {
                const href = `${prefix}/docs/commands/${command.slug}`;
                return <Link className={pathname === href ? "navLink active" : "navLink"} href={href} key={command.slug} onClick={() => setMenuOpen(false)}><code>{command.title}</code></Link>;
              })}
            </div>
          ))}
        </div>
        <div className="sidebarFooter"><span className="statusDot" /><span>{zh ? "开源 · MIT License" : "Open source · MIT License"}</span></div>
      </aside>
      {menuOpen && <button className="mobileBackdrop" type="button" onClick={() => setMenuOpen(false)} aria-label={zh ? "关闭导航" : "Close navigation"} />}

      <main className="mainContent">{children}</main>

      {searchOpen && (
        <div className="searchOverlay" role="dialog" aria-modal="true" aria-label={zh ? "搜索命令" : "Search commands"}>
          <button className="searchBackdrop" type="button" onClick={() => setSearchOpen(false)} aria-label={zh ? "关闭搜索" : "Close search"} />
          <div className="searchPanel">
            <div className="searchField">
              <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m14.3 14.3 3.2 3.2m-1.7-8.2a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0Z" /></svg>
              <input ref={inputRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={zh ? "搜索全部 Pinset 命令…" : "Search all Pinset commands…"} />
              <kbd>ESC</kbd>
            </div>
            <div className="searchResults">
              {results.map((command) => <Link href={`${prefix}/docs/commands/${command.slug}`} key={command.slug} onClick={() => setSearchOpen(false)}><code>pinset {command.title}</code><span>{command.description}</span></Link>)}
              {!results.length && <p>{zh ? "没有找到相关命令。" : "No matching commands."}</p>}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
