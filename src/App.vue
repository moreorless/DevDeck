<script setup lang="ts">
import { NConfigProvider, NMessageProvider, NDialogProvider, zhCN, dateZhCN } from "naive-ui";
import AppLayout from "./app/AppLayout.vue";
import { useThemeStore } from "./app/theme";

const theme = useThemeStore();
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
