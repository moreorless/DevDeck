/** 数字与金额格式化 */

const nf = new Intl.NumberFormat("en-US");

/** 精确值，千分位 */
export const exactNumber = (n: number) => nf.format(n);

/** token 数：≥10 亿显示 B，≥100 万显示 M，其余千分位 */
export function fmtTokens(n: number): string {
  if (Math.abs(n) >= 1e9) return `${(n / 1e9).toFixed(2)}B`;
  if (Math.abs(n) >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
  return nf.format(n);
}

/** 坐标轴刻度用：更短 */
export function fmtTokensAxis(n: number): string {
  if (n >= 1e9) return `${+(n / 1e9).toFixed(1)}B`;
  if (n >= 1e6) return `${+(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${+(n / 1e3).toFixed(1)}K`;
  return String(n);
}

const usd = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", minimumFractionDigits: 2, maximumFractionDigits: 2 });

export function fmtUsd(n: number): string {
  if (n > 0 && n < 0.01) return "<$0.01";
  return usd.format(n);
}

/** 精确金额（悬停提示用） */
export const exactUsd = (n: number) => `$${n.toFixed(6)}`;

/** 相对上一周期的变化；上一周期为 0 时无法比较，返回 null */
export function pctChange(cur: number, prev: number): number | null {
  if (prev === 0) return null;
  return ((cur - prev) / prev) * 100;
}
