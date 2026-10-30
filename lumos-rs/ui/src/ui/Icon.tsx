export const ICONS: Record<string, string> = {
  live: "M13 2 4 14h7l-1 8 9-12h-7l1-8z",
  desk: "M6 3v18M12 3v18M18 3v18M4 8h4M10 15h4M16 6h4",
  cues: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
  patch: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
  stage: "M12 2 3 7v10l9 5 9-5V7l-9-5zM3 7l9 5 9-5M12 12v10",
  plan: "M3 6l6-2 6 2 6-2v14l-6 2-6-2-6 2zM9 4v14M15 6v14",
  monitor: "M3 4h18v12H3zM8 20h8M12 16v4M7 12l3-3 3 2 4-4",
  vc: "M3 5h18v14H3zM7 9h2M11 9h2M15 9h2M7 13h2M11 13h2M15 13h2",
  timeline: "M3 12h18M7 8v8M12 5v14M17 9v6",
  script: "M6 3h9l4 4v14H6zM14 3v5h5M9 12h7M9 16h7",
  settings: "M12 15a3 3 0 100-6 3 3 0 000 6zM4 12h2M18 12h2M12 4v2M12 18v2M6.3 6.3l1.4 1.4M16.3 16.3l1.4 1.4M17.7 6.3l-1.4 1.4M7.7 16.3l-1.4 1.4",
  window: "M3 5h18v14H3zM3 9h18M7 7h.01M10 7h.01",
  fullscreen: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
  close: "M6 6l12 12M18 6 6 18",
  chevron: "M6 9l6 6 6-6",
};

export function Icon({ d, size = 26 }: { d: string; size?: number }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d={d} />
    </svg>
  );
}
