/** 本地日期工具，统一使用 YYYY-MM-DD 字符串 */

export function ymd(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function parseYmd(s: string): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function addDays(s: string, n: number): string {
  const d = parseYmd(s);
  d.setDate(d.getDate() + n);
  return ymd(d);
}

export const today = () => ymd(new Date());

/** since..until（含）的所有日期 */
export function eachDay(since: string, until: string): string[] {
  const out: string[] = [];
  for (let d = since; d <= until; d = addDays(d, 1)) out.push(d);
  return out;
}

/** MM-DD，图表横轴用 */
export const shortDate = (s: string) => s.slice(5);
