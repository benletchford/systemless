// Nearest-rank percentiles. Missing measurements remain null rather than
// appearing as zero-cost work; keep sample count alongside every distribution.
export function percentiles(values) {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  const rank = (p) => sorted.length
    ? Math.round(sorted[Math.ceil(sorted.length * p) - 1] * 10) / 10
    : null;
  return { count: sorted.length, p50: rank(0.50), p95: rank(0.95), p99: rank(0.99) };
}
