import { ImageResponse } from "next/og";
import { logoDataUri } from "@/lib/brand";

export const dynamic = "force-static";

export const alt = "Pinset — Every runtime. Right in place.";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default async function Image() {
  return new ImageResponse(
    <div style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column", justifyContent: "space-between", padding: 64, background: "#f7f7fa", color: "#20212b", borderBottom: "12px solid #4f57d8" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 18, fontSize: 34, fontWeight: 800 }}>
        <img src={await logoDataUri()} alt="" width={52} height={57} />
        pinset <span style={{ padding: "5px 12px", background: "#e9ebff", color: "#4f57d8", fontSize: 17 }}>LATEST RELEASE</span>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
        <div style={{ display: "flex", flexDirection: "column", fontSize: 86, lineHeight: 1.06, letterSpacing: "-4px", fontWeight: 800 }}><span>Every runtime.</span><span style={{ color: "#4f57d8" }}>Right in place.</span></div>
        <div style={{ fontSize: 25, color: "#50505c" }}>The polyglot runtime manager. One config. One lockfile.</div>
      </div>
      <div style={{ display: "flex", gap: 18, color: "#68758b", fontSize: 20 }}>Node.js · Python · Rust · Go · Java · .NET · Flutter · Bun</div>
    </div>,
    size,
  );
}
