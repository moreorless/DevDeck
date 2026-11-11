<script setup lang="ts">
import { onMounted } from "vue";
import { NConfigProvider, NMessageProvider, NDialogProvider, zhCN, dateZhCN } from "naive-ui";
import AppLayout from "./app/AppLayout.vue";
import { useThemeStore } from "./app/theme";
import { useSettingsStore } from "./app/settings";

const theme = useThemeStore();
const settings = useSettingsStore();

// 主题以配置文件为准
onMounted(async () => {
  try {
    await settings.load();
    if (settings.config) theme.mode = settings.config.theme;
  } catch {
    /* 非 Tauri 环境或读取失败时沿用缓存 */
  }
});
</script>

<template>
  <n-config-provider :theme="theme.naiveTheme" :locale="zhCN" :date-locale="dateZhCN">
    <n-message-provider>
      <n-dialog-provider>
        <AppLayout />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>
