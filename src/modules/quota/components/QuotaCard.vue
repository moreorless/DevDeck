<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { NCard, NButton, NProgress, NText, NAlert, NTooltip } from "naive-ui";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  getQuota,
  refreshQuota,
  onQuotaUpdated,
  windowLabel,
  fmtLocalShort,
  fmtRemaining,
  type QuotaSnapshot,
  type QuotaWindow,
} from "../api";

const snap = ref<QuotaSnapshot | null>(null);
const now = ref(Date.now());
let unlisten: UnlistenFn | null = null;
let timer: number | undefined;

onMounted(async () => {
  try {
    snap.value = await getQuota();
  } catch {
    /* 非 Tauri 环境 */
  }
  unlisten = await onQuotaUpdated((s) => (snap.value = s));
  // 打开页面时没有读数则立即读一次（后台定时任务也会读）
  if (snap.value && !snap.value.fetchedAt && !snap.value.refreshing) refresh();
  timer = window.setInterval(() => (now.value = Date.now()), 30_000);
});
onUnmounted(() => {
  unlisten?.();
  window.clearInterval(timer);
});

async function refresh() {
  try {
    snap.value = await refreshQuota();
  } catch (e) {
    if (snap.value) snap.value = { ...snap.value, error: String(e) };
  }
}

/** 70% 以下正常，70–90% 提醒，90% 以上告急 */
function status(p: number): "success" | "warning" | "error" {
  return p >= 90 ? "error" : p >= 70 ? "warning" : "success";
}

function resetLine(w: QuotaWindow): string {
  if (w.resetsAt) {
    const left = fmtRemaining(w.resetsAt, now.value);
    return left ? `${fmtLocalShort(w.resetsAt)} 重置 · ${left}后` : `${fmtLocalShort(w.resetsAt)} 已重置，等待更新`;
  }
  return w.resetsText ? `重置：${w.resetsText}` : "";
}

const fetchedText = computed(() => (snap.value?.fetchedAt ? `更新于 ${fmtLocalShort(snap.value.fetchedAt)}` : "尚未读取"));
</script>

<template>
  <n-card v-if="snap" size="small" title="套餐额度">
    <template #header-extra>
      <n-tooltip>
        <template #trigger>
          <n-text depth="3" class="meta">{{ fetchedText }}</n-text>
        </template>
        数据来自 Claude Code 的 /usage（不调用模型、不消耗额度）；有新用量时每 10 分钟左右自动更新
      </n-tooltip>
      <n-button size="tiny" quaternary :loading="snap.refreshing" @click="refresh">刷新</n-button>
    </template>

    <n-alert v-if="snap.error" type="warning" :show-icon="false" class="alert">
      读取失败{{ snap.windows.length ? "，显示的是上次的读数" : "" }}：{{ snap.error }}
    </n-alert>

    <div v-if="snap.windows.length" class="grid">
      <div v-for="w in snap.windows" :key="w.kind + (w.scope ?? '')" class="win">
        <div class="head">
          <span class="label">{{ windowLabel(w) }}</span>
          <span class="pct">{{ Math.round(w.usedPercent) }}%</span>
        </div>
        <n-progress
          type="line"
          :percentage="Math.min(100, w.usedPercent)"
          :status="status(w.usedPercent)"
          :show-indicator="false"
          :height="8"
        />
        <n-text depth="3" class="reset">{{ resetLine(w) }}</n-text>
      </div>
    </div>
    <n-text v-else-if="snap.note" depth="3" class="empty">{{ snap.note }}</n-text>
    <n-text v-else-if="!snap.error" depth="3" class="empty">
      {{ snap.refreshing ? "正在读取…" : "还没有读数，点击「刷新」读取" }}
    </n-text>
  </n-card>
</template>

<style scoped>
.meta {
  font-size: 12px;
  margin-right: 4px;
}
.alert {
  margin-bottom: 10px;
  font-size: 12px;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px 24px;
}
.win {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}
.label {
  font-size: 13px;
}
.pct {
  font-size: 20px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.reset {
  font-size: 12px;
}
.empty {
  font-size: 13px;
}
</style>
