<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  NButton,
  NEmpty,
  NSpin,
  NText,
  NAlert,
  NTag,
  NRadioGroup,
  NRadioButton,
  useMessage,
  useDialog,
} from "naive-ui";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useSettingsStore } from "../../../app/settings";
import { fmtUsd } from "../../../shared/format";
import {
  listInsights,
  syncInsights,
  getInsightHtml,
  openInsightExternal,
  deleteInsight,
  generateInsights,
  translateInsight,
  getInsightsJob,
  onInsightsUpdated,
  onInsightsJob,
  fmtLocal,
  type InsightsReport,
  type JobStatus,
} from "../api";

const settings = useSettingsStore();
const message = useMessage();
const dialog = useDialog();

const enabled = computed(() => settings.config?.insights.enabled ?? false);
const autoTranslate = computed(() => settings.config?.insights.translate ?? true);
const reports = ref<InsightsReport[]>([]);
const selectedId = ref<number | null>(null);
/** 当前显示中文译文还是原文 */
const lang = ref<"zh" | "en">("zh");
const html = ref<string | null>(null);
const loadingHtml = ref(false);
const htmlError = ref<string | null>(null);
const syncing = ref(false);
const job = ref<JobStatus | null>(null);
const now = ref(Date.now());
const unlisteners: UnlistenFn[] = [];
let timer: number | undefined;

const selected = computed(() => reports.value.find((r) => r.id === selectedId.value) ?? null);
const showZh = computed(() => lang.value === "zh" && !!selected.value?.translated);
const running = computed(() => job.value?.running ?? false);
const elapsed = computed(() => {
  if (!job.value?.startedAt) return "";
  const s = Math.max(0, Math.floor((now.value - new Date(job.value.startedAt).getTime()) / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
});

async function loadList() {
  try {
    reports.value = await listInsights();
    if (!reports.value.some((r) => r.id === selectedId.value)) selectedId.value = reports.value[0]?.id ?? null;
  } catch (e) {
    message.error(String(e));
  }
}

async function loadHtml() {
  const id = selectedId.value;
  const zh = showZh.value;
  html.value = null;
  htmlError.value = null;
  if (id == null) return;
  loadingHtml.value = true;
  try {
    const h = await getInsightHtml(id, zh);
    if (selectedId.value === id && showZh.value === zh) html.value = h;
  } catch (e) {
    if (selectedId.value === id) htmlError.value = String(e);
  } finally {
    loadingHtml.value = false;
  }
}

// 切换报告或语言、或译文刚生成时重新加载
watch([selectedId, showZh], loadHtml);
watch(selectedId, () => (lang.value = "zh"));

function onJob(s: JobStatus) {
  const wasRunning = job.value?.running;
  job.value = s;
  if (wasRunning && !s.running) {
    if (s.error) message.error(s.error, { duration: 8000 });
    else message.success(`${s.message ?? "完成"}${s.costUsd ? `（估算费用 ${fmtUsd(s.costUsd)}）` : ""}`);
    if (s.reportId != null) selectedId.value = s.reportId;
  }
}

onMounted(async () => {
  if (!settings.view) await settings.load().catch(() => {});
  await loadList();
  unlisteners.push(await onInsightsUpdated(loadList), await onInsightsJob(onJob));
  job.value = await getInsightsJob().catch(() => null);
  timer = window.setInterval(() => (now.value = Date.now()), 1000);
});
onUnmounted(() => {
  unlisteners.forEach((f) => f());
  window.clearInterval(timer);
});

async function checkNow() {
  syncing.value = true;
  try {
    const n = await syncInsights();
    message.success(n ? `归档了 ${n} 份新报告` : "没有新报告");
    await loadList();
  } catch (e) {
    message.error(String(e));
  } finally {
    syncing.value = false;
  }
}

function confirmGenerate() {
  dialog.warning({
    title: "生成新的 Insights 报告",
    content:
      "将在后台运行 Claude Code 的 /insights：把本机近期会话内容发送给模型分析，消耗你的套餐额度或 API 费用" +
      "（本机实测一次约 $1.7，按 API 价格估算），通常需要 1–3 分钟。" +
      (autoTranslate.value ? "完成后再翻译为中文（约 $0.2）。" : ""),
    positiveText: "开始生成",
    negativeText: "取消",
    onPositiveClick: async () => {
      try {
        await generateInsights();
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}

function confirmTranslate() {
  const r = selected.value;
  if (!r) return;
  dialog.info({
    title: "翻译为中文",
    content: "Claude Code 原生报告只有英文。将调用 claude 把报告正文翻译为简体中文（约 $0.2、30 秒左右），原文保留。",
    positiveText: "翻译",
    negativeText: "取消",
    onPositiveClick: async () => {
      try {
        await translateInsight(r.id);
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}

async function openExternal() {
  if (selectedId.value == null) return;
  try {
    await openInsightExternal(selectedId.value, showZh.value);
  } catch (e) {
    message.error(String(e));
  }
}

function confirmDelete() {
  const r = selected.value;
  if (!r) return;
  dialog.warning({
    title: "删除报告",
    content: `删除 ${fmtLocal(r.generatedAt)} 生成的报告（含中文译文）？删除后不会再次归档同一份报告。`,
    positiveText: "删除",
    negativeText: "取消",
    onPositiveClick: async () => {
      try {
        await deleteInsight(r.id);
        await loadList();
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}

const sessionsText = (r: InsightsReport) =>
  r.sessionsAnalyzed != null
    ? `已分析 ${r.sessionsAnalyzed}${r.sessionsTotal != null ? ` / ${r.sessionsTotal}` : ""} 个会话`
    : "会话数未知";
</script>

<template>
  <div v-if="!enabled && !reports.length" class="off">
    <n-empty description="Insights 模块未开启">
      <template #extra>
        <n-text depth="3" style="font-size: 13px">
          在「设置 → Insights」中开启后，DevDeck 会归档 Claude Code 生成的 /insights 报告
        </n-text>
        <div style="margin-top: 12px"><n-button size="small" @click="$router.push('/settings')">去设置</n-button></div>
      </template>
    </n-empty>
  </div>

  <div v-else class="layout">
    <aside class="left">
      <div class="head">
        <span class="title">Insights 报告</span>
        <n-button v-if="enabled" size="tiny" quaternary :loading="syncing" @click="checkNow">检查新报告</n-button>
      </div>
      <n-button
        v-if="enabled"
        type="primary"
        size="small"
        block
        :loading="running"
        :disabled="running"
        style="margin-bottom: 8px"
        @click="confirmGenerate"
      >
        {{ running ? (job?.phase === "translating" ? "正在翻译" : "正在生成") : "生成新报告" }}
      </n-button>
      <n-alert v-if="running" type="info" :show-icon="false" class="job">
        {{ job?.phase === "translating" ? "正在翻译为中文" : "正在后台运行 /insights，通常需要 1–3 分钟" }}
        · 已用 {{ elapsed }}
        <div class="hint">可以离开此页面，完成后自动出现在列表中</div>
      </n-alert>
      <n-alert
        v-else-if="job?.error"
        type="error"
        :show-icon="false"
        class="job"
        closable
        @close="job = { ...job!, error: null }"
      >
        {{ job.error }}
      </n-alert>
      <n-alert v-if="!enabled" type="info" :show-icon="false" class="job">
        模块已关闭，不再归档新报告；已归档的报告仍可查看
      </n-alert>
      <div class="list">
        <div
          v-for="r in reports"
          :key="r.id"
          class="item"
          :class="{ active: r.id === selectedId }"
          @click="selectedId = r.id"
        >
          <div class="time">
            {{ fmtLocal(r.generatedAt) }}
            <n-tag v-if="r.translated" size="tiny" :bordered="false" type="success">中文</n-tag>
          </div>
          <n-text depth="3" class="sub">{{ sessionsText(r) }}</n-text>
          <n-text v-if="r.periodStart" depth="3" class="sub">{{ r.periodStart }} ~ {{ r.periodEnd }}</n-text>
          <n-text v-if="r.costUsd" depth="3" class="sub">DevDeck 生成费用 {{ fmtUsd(r.costUsd) }}</n-text>
        </div>
        <div v-if="!reports.length" class="empty">
          <div>还没有报告</div>
          <n-text depth="3" style="font-size: 12px">点击「生成新报告」，或在 Claude Code 中运行 /insights</n-text>
        </div>
      </div>
    </aside>

    <section class="right">
      <template v-if="selected">
        <div class="toolbar">
          <n-radio-group v-if="selected.translated" v-model:value="lang" size="small">
            <n-radio-button value="zh">中文</n-radio-button>
            <n-radio-button value="en">原文</n-radio-button>
          </n-radio-group>
          <n-button v-else-if="enabled" size="small" :disabled="running" @click="confirmTranslate">翻译为中文</n-button>
          <n-text depth="3" class="src" :title="selected.sourcePath">来源：{{ selected.sourcePath }}</n-text>
          <n-button size="small" @click="openExternal">用浏览器打开</n-button>
          <n-button size="small" @click="confirmDelete">删除</n-button>
        </div>
        <div class="frame-wrap">
          <n-spin v-if="loadingHtml" style="margin: 60px auto; display: block" />
          <n-alert v-else-if="htmlError" type="error" :title="htmlError" />
          <!-- 报告自带折叠等脚本：允许脚本，但不给同源权限，无法访问 DevDeck 页面与 Tauri 接口 -->
          <iframe v-else-if="html" class="frame" sandbox="allow-scripts" :srcdoc="html" title="Insights 报告" />
        </div>
      </template>
    </section>
  </div>
</template>

<style scoped>
/* 布局内容区上下各有 20px 内边距；两栏各自滚动 */
.layout {
  display: flex;
  gap: 16px;
  height: calc(100vh - 40px);
}
.left {
  width: 260px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-right: 1px solid rgba(128, 128, 128, 0.2);
  padding-right: 12px;
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}
.title {
  font-size: 16px;
  font-weight: 600;
}
.job {
  margin-bottom: 8px;
  font-size: 12px;
}
.hint {
  opacity: 0.75;
  margin-top: 2px;
}
.list {
  flex: 1;
  overflow-y: auto;
}
.item {
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.item:hover {
  background: rgba(128, 128, 128, 0.1);
}
.item.active {
  background: rgba(22, 163, 74, 0.14);
}
.time {
  font-size: 14px;
  font-weight: 500;
  display: flex;
  align-items: center;
  gap: 6px;
}
.sub {
  font-size: 12px;
}
.empty {
  margin-top: 40px;
  text-align: center;
  font-size: 13px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.right {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}
.src {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.frame-wrap {
  flex: 1;
  min-height: 0;
}
.frame {
  width: 100%;
  height: 100%;
  border: 1px solid rgba(128, 128, 128, 0.25);
  border-radius: 6px;
  /* 报告自身是浅色设计，深色主题下也保持白底 */
  background: #fff;
}
.off {
  padding-top: 15vh;
}
</style>
