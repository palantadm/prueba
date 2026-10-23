// Fan / spread helpers: distribute an attribute across an ordered list of fixtures.
export type FanMode = "linear" | "center" | "random";

/** Deterministic PRNG so a fan preview matches what gets applied. */
function rnd(seed: number) {
  let s = (seed >>> 0) || 1;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967296;
  };
}

/**
 * Values (0..1) for `n` fixtures between `from` and `to`.
 * linear: first→last; center: middle at `from`, outermost at `to`; random: seeded values in range.
 * `reverse` flips the order of the result.
 */
export function fanValues(n: number, from: number, to: number, mode: FanMode, reverse: boolean, seed = 1): number[] {
  if (n <= 0) return [];
  const clamp = (v: number) => Math.max(0, Math.min(1, v));
  let out: number[];
  if (n === 1) out = [from];
  else if (mode === "linear") out = Array.from({ length: n }, (_, i) => from + (to - from) * (i / (n - 1)));
  else if (mode === "center") {
    const mid = (n - 1) / 2;
    out = Array.from({ length: n }, (_, i) => from + (to - from) * (mid === 0 ? 0 : Math.abs(i - mid) / mid));
  } else {
    const r = rnd(seed * 2654435761);
    out = Array.from({ length: n }, () => from + (to - from) * r());
  }
  out = out.map(clamp);
  return reverse ? out.reverse() : out;
}
