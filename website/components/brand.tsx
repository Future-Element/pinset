import Link from "next/link";

export function Brand({ href = "/" }: { href?: string }) {
  return (
    <Link className="brand" href={href} aria-label="Pinset home">
      <svg className="brandMark" viewBox="0 0 36 36" aria-hidden="true">
        <path d="M9 7h10.2c5.1 0 8.5 3 8.5 7.6s-3.4 7.7-8.5 7.7h-5.1V29H9V7Z" />
        <path className="brandCut" d="M14.1 11.5h4.7c2.6 0 3.9 1 3.9 3.1 0 2.1-1.3 3.2-3.9 3.2h-4.7v-6.3Z" />
        <circle cx="27.5" cy="27.5" r="3.4" />
      </svg>
      <span>pinset</span>
      <small>2.1</small>
    </Link>
  );
}
