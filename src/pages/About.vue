<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import {
  NH2,
  NH3,
  NDescriptions,
  NDescriptionsItem,
  NButton,
  NSpace,
  NProgress,
  NAlert,
  useMessage,
  useDialog,
} from "naive-ui";
import { getVersion } from "@tauri-apps/api/app";
import { getScanStatus, onDataEvents, refresh, type ScanMode, type ScanStatus } from "../shared/data";

const message = useMessage();
const dialog = useDialog();
const version = ref("-");
const status = ref<ScanStatus | null>(null);
const progress = ref<{ done: number; total: number } | null>(null);
const busy = ref(false);
let unlisten: (() => void) | null = null;

async function load() {
  try {
    status.value = await getScanStatus();
  } catch (e) {
    message.error(String(e));
  }
}

async function run(mode: ScanMode) {
  busy.value = true;
  try {
    const s = await refresh(mode);
    message.success(`扫描完成：读取 ${s.filesRead}/${s.filesTotal} 个文件，耗时 ${s.durationMs} ms`);
  } catch (e) {
    message.error(String(e));
  } finally {
    busy.value = false;
    progress.value = null;
    await load();
  }
}

function confirmFull() {
  dialog.warning({
    title: "全量重建缓存",
    content: "将重新读取全部日志，期间旧数据仍可查看。确定继续？",
    positiveText: "重建",
    negativeText: "取消",
    onPositiveClick: () => run("full"),
  });
}

const fmtTime = (s?: string | null) => (s ? new Date(s).toLocaleString() : "-");

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    version.value = "开发模式（非 Tauri 环境）";
  }
  await load();
  unlisten = await onDataEvents({
    updated: () => load(),
    error: () => load(),
    progress: (p) => (progress.value = p),
  });
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <n-h2>关于</n-h2>
  <n-descriptions :column="1" label-placement="left">
    <n-descriptions-item label="应用">DevDeck（开发台）</n-descriptions-item>
    <n-descriptions-item label="版本">{{ version }}</n-descriptions-item>
    <n-descriptions-item label="数据">仅读取本机 Claude Code 日志，不联网、不上传</n-descriptions-item>
  </n-descriptions>

  <n-h3>数据状态</n-h3>
  <template v-if="status">
    <n-alert v-if="status.foundDirs.length === 0" type="warning" title="未找到 Claude 日志目录" style="margin-bottom: 12px">
      已检查：{{ status.checkedDirs.join("；") }}
    </n-alert>
    <n-alert v-else-if="status.lastScan && !status.lastScan.ok" type="error" title="上次扫描失败" style="margin-bottom: 12px">
      {{ status.lastScan.error }}
    </n-alert>
    <n-descriptions :column="1" label-placement="left">
      <n-descriptions-item label="日志目录">{{ status.foundDirs.join("；") || "-" }}</n-descriptions-item>
      <n-descriptions-item label="会话数">{{ status.sessionCount.toLocaleString() }}</n-descriptions-item>
      <n-descriptions-item label="用量记录">{{ status.usageCount.toLocaleString() }}</n-descriptions-item>
      <n-descriptions-item label="上次扫描">
        {{ fmtTime(status.lastScan?.finishedAt) }}
        <template v-if="status.lastScan?.ok">
          （{{ status.lastScan.mode === "full" ? "全量" : "增量" }}，{{ status.lastScan.filesTotal }} 个文件，坏行
          {{ status.lastScan.badLines }}）
        </template>
      </n-descriptions-item>
      <n-descriptions-item label="价格表核对日期">{{ status.pricingCheckedAt }}（费用均为估算）</n-descriptions-item>
    </n-descriptions>
  </template>
  <n-progress
    v-if="progress"
    type="line"
    :percentage="Math.round((progress.done / progress.total) * 100)"
    style="margin: 12px 0; max-width: 480px"
  />
  <n-space style="margin-top: 12px">
    <n-button :loading="busy || status?.scanning" @click="run('incremental')">立即刷新</n-button>
    <n-button :disabled="busy || status?.scanning" @click="confirmFull">全量重建缓存</n-button>
  </n-space>
</template>
