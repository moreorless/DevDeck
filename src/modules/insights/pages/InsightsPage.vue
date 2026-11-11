<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { NButton, NEmpty, NSpin, NText, NAlert, useMessage, useDialog } from "naive-ui";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useSettingsStore } from "../../../app/settings";
import {
  listInsights,
  syncInsights,
  getInsightHtml,
  openInsightExternal,
  deleteInsight,
  onInsightsUpdated,
  fmtLocal,
  type InsightsReport,
} from "../api";

const settings = useSettingsStore();
const message = useMessage();
const dialog = useDialog();

const enabled = computed(() => settings.config?.insights.enabled ?? false);
const reports = ref<InsightsReport[]>([]);
const selectedId = ref<number | null>(null);
const html = ref<string | null>(null);
const loadingHtml = ref(false);
const htmlError = ref<string | null>(null);
const syncing = ref(false);
let unlisten: UnlistenFn | null = null;

const selected = computed(() => reports.value.find((r) => r.id === selectedId.value) ?? null);

async function loadList() {
  try {
    reports.value = await listInsights();
    if (!reports.value.some((r) => r.id === selectedId.value)) selectedId.value = reports.value[0]?.id ?? null;
  } catch (e) {
    message.error(String(e));
  }
}

watch(selectedId, async (id) => {
  html.value = null;
  htmlError.value = null;
  if (id == null) return;
  loadingHtml.value = true;
  try {
    const h = await getInsightHtml(id);
    if (selectedId.value === id) html.value = h;
  } catch (e) {
    if (selectedId.value === id) htmlError.value = String(e);
  } finally {
    loadingHtml.value = false;
  }
});

onMounted(async () => {
  if (!settings.view) await settings.load().catch(() => {});
  await loadList();
  unlisten = await onInsightsUpdated(loadList);
});
onUnmounted(() => unlisten?.());

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

async function openExternal() {
  if (selectedId.value == null) return;
  try {
    await openInsightExternal(selectedId.value);
  } catch (e) {
    message.error(String(e));
  }
}

function confirmDelete() {
  const r = selected.value;
  if (!r) return;
  dialog.warning({
    title: "删除报告",
    content: `删除 ${fmtLocal(r.generatedAt)} 生成的报告？删除后不会再次归档同一份报告。`,
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
        <n-button v-if="enabled" size="tiny" :loading="syncing" @click="checkNow">检查新报告</n-button>
      </div>
      <n-alert v-if="!enabled" type="info" :show-icon="false" style="margin-bottom: 8px; font-size: 12px">
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
          <div class="time">{{ fmtLocal(r.generatedAt) }}</div>
          <n-text depth="3" class="sub">{{ sessionsText(r) }}</n-text>
          <n-text v-if="r.periodStart" depth="3" class="sub">{{ r.periodStart }} ~ {{ r.periodEnd }}</n-text>
        </div>
        <div v-if="!reports.length" class="empty">
          <div>还没有报告</div>
          <n-text depth="3" style="font-size: 12px">在 Claude Code 中运行 /insights，生成后会自动归档</n-text>
        </div>
      </div>
    </aside>

    <section class="right">
      <template v-if="selected">
        <div class="toolbar">
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
