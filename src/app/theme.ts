import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { darkTheme, useOsTheme, type GlobalTheme } from "naive-ui";

export type ThemeMode = "system" | "light" | "dark";

/** localStorage 只做首屏缓存（避免启动闪烁），配置以 config.json 为准 */
const STORAGE_KEY = "devdeck.theme";

function loadCached(): ThemeMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* 存储不可用时使用默认值 */
  }
  return "system";
}

export const useThemeStore = defineStore("theme", () => {
  const mode = ref<ThemeMode>(loadCached());
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
