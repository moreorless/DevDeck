<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { NButton, NForm, NFormItem, NSwitch, NText, useDialog, useMessage } from "naive-ui";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useSettingsStore } from "../../app/settings";
import { clearInsights, detectClaude, listInsights, onInsightsUpdated } from "../../modules/insights/api";

const settings = useSettingsStore();
const message = useMessage();
const dialog = useDialog();
const count = ref(0);
let unlisten: UnlistenFn | null = null;

const enabled = computed(() => settings.config?.insights.enabled ?? false);
const insights = computed(() => settings.config!.insights);
/** 实际会使用的 claude 路径，或找不到时的错误 */
const claude = ref<{ path?: string; error?: string }>({});

async function loadClaude() {
  try {
    claude.value = { path: await detectClaude() };
  } catch (e) {
    claude.value = { error: String(e) };
  }
}
watch(() => settings.config?.insights.claudePath, loadClaude);

async function loadCount() {
  try {
    count.value = (await listInsights()).length;
  } catch {
    /* 忽略：设置页其他部分照常可用 */
  }
}

onMounted(async () => {
  await Promise.all([loadCount(), loadClaude()]);
  unlisten = await onInsightsUpdated(loadCount);
});
onUnmounted(() => unlisten?.());

async function patch(p: Partial<typeof insights.value>, ok?: string) {
  try {
    await settings.save({ insights: { ...insights.value, ...p } });
    if (ok) message.success(ok);
  } catch (e) {
    message.error(String(e));
  }
}
const save = (on: boolean) =>
  patch({ enabled: on }, on ? "已开启 Insights，正在归档已有报告" : "已关闭 Insights，已归档的报告保留");

async function pickClaude() {
  const picked = await open({ title: "选择 claude 可执行文件", filters: [{ name: "claude", extensions: ["exe", "cmd"] }] });
  if (typeof picked === "string") await patch({ claudePath: picked }, "已指定 claude 路径");
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
        归档并查看 Claude Code 原生 /insights 报告；可在 Insights 页一键生成，或在 Claude Code 中运行 /insights（均消耗额度）
      </n-text>
    </n-form-item>
    <n-form-item label="翻译为中文">
      <n-switch :value="insights.translate" @update:value="(v: boolean) => patch({ translate: v })" />
      <n-text depth="3" style="font-size: 12px; margin-left: 12px">
        原生报告只有英文；从 DevDeck 生成后自动调用 claude 翻译正文（约 $0.2/份），原文保留
      </n-text>
    </n-form-item>
    <n-form-item label="claude 程序">
      <n-text v-if="claude.path" class="path">{{ claude.path }}</n-text>
      <n-text v-else type="warning" style="font-size: 12px">{{ claude.error }}</n-text>
      <n-text depth="3" style="font-size: 12px; margin-left: 8px">{{ insights.claudePath ? "手动指定" : "自动检测" }}</n-text>
      <n-button size="tiny" style="margin-left: 8px" @click="pickClaude">选择…</n-button>
      <n-button v-if="insights.claudePath" size="tiny" quaternary @click="patch({ claudePath: null })">改回自动检测</n-button>
    </n-form-item>
    <n-form-item label="已归档">
      <n-text>{{ count }} 份</n-text>
      <n-button size="small" style="margin-left: 12px" :disabled="!count" @click="confirmClear">清除全部归档</n-button>
    </n-form-item>
  </n-form>
</template>

<style scoped>
.path {
  font-family: Consolas, "Cascadia Mono", monospace;
  font-size: 13px;
}
</style>
