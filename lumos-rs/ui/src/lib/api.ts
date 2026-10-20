// Tolerant helpers for the small REST surface.
export type PortInfo = { name: string; desc: string };

export async function fetchPorts(): Promise<PortInfo[]> {
  const r = await fetch("/api/ports");
  if (!r.ok) throw new Error(`/api/ports ${r.status}`);
  const j = await r.json();
  const arr: unknown[] = Array.isArray(j) ? j : (j.ports ?? []);
  return arr.map((x: any) =>
    typeof x === "string" ? { name: x, desc: "" }
    : Array.isArray(x) ? { name: String(x[0]), desc: String(x[1] ?? "") }
    : { name: String(x.name ?? x.port), desc: String(x.description ?? x.desc ?? "") });
}

export async function fetchNet(): Promise<string[]> {
  const r = await fetch("/api/net");
  if (!r.ok) throw new Error(`/api/net ${r.status}`);
  const j = await r.json();
  const arr: unknown[] = Array.isArray(j) ? j : (j.ips ?? j.addresses ?? j.interfaces ?? []);
  return arr.map((x: any) => (typeof x === "string" ? x : String(x.ip ?? x.addr ?? x.address ?? (Array.isArray(x) ? x[1] : ""))))
    .filter((s) => s && !s.startsWith("127.") && s !== "::1");
}

export async function downloadShow(name: string) {
  const r = await fetch("/api/show");
  const blob = await r.blob();
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `${name || "show"}.json`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 2000);
}

export async function importShow(file: File) {
  const r = await fetch("/api/show", { method: "PUT", headers: { "content-type": "application/json" }, body: await file.text() });
  if (!r.ok) throw new Error(`Import failed (${r.status}) ${await r.text().catch(() => "")}`);
}
