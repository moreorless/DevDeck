/**
 * 图表配色：dataviz 参考调色板（references/palette.md），浅色/深色各自选定的步进。
 * 类别色按固定顺序分配，不循环；第 8 类以后归入「其他」。
 */

const CATEGORICAL = {
  light: ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"],
  dark: ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"],
};

const CHROME = {
  light: {
    surface: "#fcfcfb",
    textPrimary: "#0b0b0b",
    textSecondary: "#52514e",
    muted: "#898781",
    grid: "#e1e0d9",
    axis: "#c3c2b7",
    tooltipBg: "#ffffff",
    border: "rgba(11,11,11,0.10)",
  },
  dark: {
    surface: "#1a1a19",
    textPrimary: "#f0efec",
    textSecondary: "#c3c2b7",
    muted: "#898781",
    grid: "#2c2c2a",
    axis: "#383835",
    tooltipBg: "#262624",
    border: "rgba(255,255,255,0.10)",
  },
};

/** 「其他」用中性灰，不占类别色 */
const OTHER = { light: "#a8a69f", dark: "#6b6a64" };

export const MAX_CATEGORIES = 7;

export function chartTheme(dark: boolean) {
  const mode = dark ? "dark" : "light";
  return { ...CHROME[mode], categorical: CATEGORICAL[mode], other: OTHER[mode] };
}

/**
 * 实体 → 颜色：按实体在「全部实体」中的固定位置分配，筛选不会让剩下的实体换色。
 * 位置 ≥ MAX_CATEGORIES 的实体返回 null，调用方应归入「其他」。
 */
export function colorFor(entity: string, allEntities: string[], dark: boolean): string | null {
  const i = allEntities.indexOf(entity);
  if (i < 0 || i >= MAX_CATEGORIES) return null;
  return chartTheme(dark).categorical[i];
}
