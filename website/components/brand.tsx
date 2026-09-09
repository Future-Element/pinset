import Link from "next/link";

export function Brand({ href = "/" }: { href?: string }) {
  return (
    <Link className="brand" href={href} aria-label="Pinset home">
      <img className="brandMark" src="/brand/pinset-mark.svg" width="32" height="35" alt="" />
      <span>pinset</span>
    </Link>
  );
}
