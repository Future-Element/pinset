import { ImageResponse } from "next/og";

export const dynamic = "force-static";

export const alt = "Pinset — One project. One runtime set.";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function Image() {
  return new ImageResponse(
    <div style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column", justifyContent: "space-between", padding: 72, background: "#f8fafc", color: "#10182b", borderTop: "12px solid #159c91" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 18, fontSize: 34, fontWeight: 800 }}>
        <div style={{ width: 52, height: 52, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 13, background: "#4f57d8", color: "white" }}>P.</div>
        pinset <span style={{ padding: "5px 12px", borderRadius: 20, background: "#e9ebff", color: "#4f57d8", fontSize: 17 }}>2.1</span>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
        <div style={{ display: "flex", flexDirection: "column", fontSize: 76, lineHeight: 1.04, letterSpacing: "-4px", fontWeight: 800 }}><span>One project.</span><span>One runtime set.</span></div>
        <div style={{ fontSize: 26, color: "#536078" }}>Predictable polyglot runtimes, locked to the project.</div>
      </div>
      <div style={{ display: "flex", gap: 18, color: "#68758b", fontSize: 20 }}>Node.js · Python · Rust · Go · Java · .NET · Flutter · Bun</div>
    </div>,
    size,
  );
}
