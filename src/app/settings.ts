import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface Rates {
  input: number;
  cacheWrite5m: number;
  cacheWrite1h: number;
  cacheRead: number;
  output: number;
}

export interface AppConfig {
  claudeDirs: string[];
  timezone: string | null;
  refreshIntervalMin: number;
  pricingOverrides: Record<string, Rates>;
  autostart: boolean;
  closeToTray: boolean;
  theme: "system" | "light" | "dark";
  insights: { enabled: boolean };
}

export interface DirStatus {
  path: string;
  exists: boolean;
  custom: boolean;
}

export interface SettingsView {
  config: AppConfig;
  configError: string | null;
  dirs: DirStatus[];
  systemTimezone: string;
}

export interface PriceRow {
  model: string;
  builtin: Rates | null;
  override: Rates | null;
  longContext: (Rates & { threshold: number }) | null;
  inUse: boolean;
}

export interface PricingView {
  source: string;
  checkedAt: string;
  rows: PriceRow[];
}

export const getSettings = () => invoke<SettingsView>("get_settings");
export const setConfig = (config: AppConfig) => invoke<AppConfig>("set_config", { config });
export const listTimezones = () => invoke<string[]>("list_timezones");
export const getPricing = () => invoke<PricingView>("get_pricing");
export const checkClaudeDir = (path: string) => invoke<boolean>("check_claude_dir", { path });

/** 应用配置（Rust 侧持久化到 config.json） */
export const useSettingsStore = defineStore("settings", () => {
  const view = ref<SettingsView | null>(null);
  const config = computed(() => view.value?.config ?? null);

  async function load() {
    view.value = await getSettings();
  }

  /** 合并修改并保存；失败时抛出，界面负责提示 */
  async function save(patch: Partial<AppConfig>) {
    if (!view.value) await load();
    const next = { ...view.value!.config, ...patch };
    const saved = await setConfig(next);
    view.value = { ...view.value!, config: saved };
    // 目录变化后重新取目录状态
    if (patch.claudeDirs) await load();
    return saved;
  }

  return { view, config, load, save };
});
