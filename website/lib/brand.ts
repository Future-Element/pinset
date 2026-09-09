import { readFile } from "node:fs/promises";
import path from "node:path";

// Metadata images use the same logo asset as the navigation.
export async function logoDataUri() {
  const svg = await readFile(path.join(process.cwd(), "public", "brand", "pinset-mark.svg"));
  return `data:image/svg+xml;base64,${svg.toString("base64")}`;
}
