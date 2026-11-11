import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { darkTheme, useOsTheme, type GlobalTheme } from "naive-ui";

export type ThemeMode = "system" | "light" | "dark";

const STORAGE_KEY = "devdeck.theme";

function loadMode(): ThemeMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* 存储不可用时使用默认值 */
  }
  return "system";
}

/**
 * 主题状态。M1 暂存在 localStorage；M5 接入 tauri store 后迁移到统一配置。
 */
export const useThemeStore = defineStore("theme", () => {
  const mode = ref<ThemeMode>(loadMode());
  const osTheme = useOsTheme();

  const isDark = computed(() =>
    mode.value === "system" ? osTheme.value === "dark" : mode.value === "dark",
  );
  const naiveTheme = computed<GlobalTheme | null>(() => (isDark.value ? darkTheme : null));

  watch(mode, (v) => {
    try {
      localStorage.setItem(STORAGE_KEY, v);
    } catch {
      /* 忽略 */
    }
  });

  watch(
    isDark,
    (dark) => document.documentElement.setAttribute("data-theme", dark ? "dark" : "light"),
    { immediate: true },
  );

  return { mode, isDark, naiveTheme };
});
