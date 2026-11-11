<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { NButton, NForm, NFormItem, NSwitch, NText, useDialog, useMessage } from "naive-ui";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useSettingsStore } from "../../app/settings";
import { clearInsights, listInsights, onInsightsUpdated } from "../../modules/insights/api";

const settings = useSettingsStore();
const message = useMessage();
const dialog = useDialog();
const count = ref(0);
let unlisten: UnlistenFn | null = null;

const enabled = computed(() => settings.config?.insights.enabled ?? false);

async function loadCount() {
  try {
    count.value = (await listInsights()).length;
  } catch {
    /* 忽略：设置页其他部分照常可用 */
  }
}

onMounted(async () => {
  await loadCount();
  unlisten = await onInsightsUpdated(loadCount);
});
onUnmounted(() => unlisten?.());

async function save(on: boolean) {
  try {
    await settings.save({ insights: { ...settings.config!.insights, enabled: on } });
    message.success(on ? "已开启 Insights，正在归档已有报告" : "已关闭 Insights，已归档的报告保留");
  } catch (e) {
    message.error(String(e));
  }
}

function onToggle(on: boolean) {
  if (!on) return save(false);
  dialog.info({
    title: "开启 Insights",
    content:
      "DevDeck 会把 Claude Code /insights 生成的报告复制到本机的 DevDeck 数据目录中长期保存。" +
      "报告是模型对你会话的总结，包含项目名与工作内容摘要；DevDeck 不会上传，可随时一键清除。",
    positiveText: "开启",
    negativeText: "取消",
    onPositiveClick: () => save(true),
  });
}

function confirmClear() {
  dialog.warning({
    title: "清除全部归档",
    content: `删除 DevDeck 中归档的 ${count.value} 份 Insights 报告？Claude Code 目录下的原文件不受影响，已清除的报告不会再次归档。`,
    positiveText: "清除",
    negativeText: "取消",
    onPositiveClick: async () => {
      try {
        const n = await clearInsights();
        message.success(`已清除 ${n} 份报告`);
        await loadCount();
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}
</script>

<template>
  <n-form label-placement="left" label-width="110">
    <n-form-item label="启用">
      <n-switch :value="enabled" @update:value="onToggle" />
      <n-text depth="3" style="font-size: 12px; margin-left: 12px">
        归档并查看 Claude Code 原生 /insights 报告；报告需在 Claude Code 中运行 /insights 生成（消耗额度）
      </n-text>
    </n-form-item>
    <n-form-item label="已归档">
      <n-text>{{ count }} 份</n-text>
      <n-button size="small" style="margin-left: 12px" :disabled="!count" @click="confirmClear">清除全部归档</n-button>
    </n-form-item>
  </n-form>
</template>
