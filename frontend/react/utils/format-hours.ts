/** Integration time in hours as a short label, e.g. "5h 10m", "45m", "<1m" */
export function formatHours(hours: number): string {
  const total = Math.round(hours * 60);
  if (total < 1) return '<1m';
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (h === 0) return `${m}m`;
  return m ? `${h}h ${m}m` : `${h}h`;
}
