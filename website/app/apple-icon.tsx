import { ImageResponse } from "next/og";
import { logoDataUri } from "@/lib/brand";

export const dynamic = "force-static";
export const size = { width: 180, height: 180 };
export const contentType = "image/png";

export default async function AppleIcon() {
  return new ImageResponse(
    <div style={{ display: "flex", width: "100%", height: "100%", background: "#fff", alignItems: "center", justifyContent: "center" }}>
      <img src={await logoDataUri()} alt="" width={112} height={123} />
    </div>,
    size,
  );
}
