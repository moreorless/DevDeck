<script setup lang="ts">
import { onMounted, ref } from "vue";
import { NH2, NDescriptions, NDescriptionsItem } from "naive-ui";
import { getVersion } from "@tauri-apps/api/app";

const version = ref("-");
onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    version.value = "开发模式（非 Tauri 环境）";
  }
});
</script>

<template>
  <n-h2>关于</n-h2>
  <n-descriptions :column="1" label-placement="left">
    <n-descriptions-item label="应用">DevDeck（开发台）</n-descriptions-item>
    <n-descriptions-item label="版本">{{ version }}</n-descriptions-item>
    <n-descriptions-item label="数据">仅读取本机 Claude Code 日志，不联网、不上传</n-descriptions-item>
  </n-descriptions>
</template>
