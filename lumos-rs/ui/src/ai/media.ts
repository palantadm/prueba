// Image helpers for the assistant: downscale before upload so a phone photo doesn't become 12 MB of base64.
import type { Thumb } from "./state";

export const MAX_SIDE = 1568;
export const MAX_IMAGES = 4;

/** Decode, scale the long side to ≤ 1568 px and re-encode as JPEG (q 0.85). */
export async function fileToThumb(file: File): Promise<Thumb> {
  const bmp = await createImageBitmap(file);
  try {
    const k = Math.min(1, MAX_SIDE / Math.max(bmp.width, bmp.height));
    const w = Math.max(1, Math.round(bmp.width * k));
    const h = Math.max(1, Math.round(bmp.height * k));
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const g = c.getContext("2d");
    if (!g) throw new Error("canvas");
    g.fillStyle = "#fff"; // transparent PNGs become white, not black
    g.fillRect(0, 0, w, h);
    g.drawImage(bmp, 0, 0, w, h);
    const url = c.toDataURL("image/jpeg", 0.85);
    return { media_type: "image/jpeg", data: url.slice(url.indexOf(",") + 1), url };
  } finally {
    bmp.close();
  }
}

export const isImage = (f: File) => f.type.startsWith("image/");
