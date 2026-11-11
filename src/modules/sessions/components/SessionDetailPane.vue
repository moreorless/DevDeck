<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { NButton, NSpace, NSwitch, NEmpty, NSpin, NAlert, NBreadcrumb, NBreadcrumbItem, NTag, useMessage } from "naive-ui";
import { useSessionsStore } from "../store";
import { openProjectDir } from "../api";
import MessageItem from "./MessageItem.vue";
import { exactNumber, fmtTokens, fmtUsd } from "../../../shared/format";

const store = useSessionsStore();
const message = useMessage();
const scroller = ref<HTMLElement | null>(null);

/** 分批渲染：长会话可能有上千条，先渲染一部分，滚到底部再追加 */
const BATCH = 200;
const limit = ref(BATCH);

const visibleItems = computed(() => {
  const all = store.current?.items ?? [];
  return store.showMeta ? all : all.filter((i) => i.kind !== "meta");
});
const shown = computed(() => visibleItems.value.slice(0, limit.value));
const hiddenMeta = computed(() => (store.current?.items ?? []).filter((i) => i.kind === "meta").length);

watch(
  () => store.current,
  async () => {
    limit.value = BATCH;
    await nextTick();
    scroller.value?.scrollTo({ top: 0 });
  },
);

function onScroll() {
  const el = scroller.value;
  if (el && el.scrollTop + el.clientHeight > el.scrollHeight - 600 && limit.value < visibleItems.value.length) {
    limit.value += BATCH;
  }
}

const s = computed(() => store.selected);
const fmt = (t: string | null | undefined) => (t ? new Date(t).toLocaleString() : "-");

async function copyResume() {
  if (!s.value) return;
  try {
    await navigator.clipboard.writeText(`claude --resume ${s.value.sessionId}`);
    message.success("已复制。需要在项目目录下执行");
  } catch (e) {
    message.error(`复制失败：${e}`);
  }
}

async function openDir() {
  if (!s.value) return;
  try {
    await openProjectDir(s.value.sessionId);
  } catch (e) {
    message.error(String(e));
  }
}

/** 链接点击不在 WebView 内跳转 */
function blockLinks(e: MouseEvent) {
  if ((e.target as HTMLElement).closest("a")) e.preventDefault();
}
</script>

<template>
  <div class="pane">
    <n-empty v-if="!store.selectedId" description="从左侧选择一个会话" style="margin-top: 120px" />

    <template v-else>
      <div class="header">
        <div class="title">{{ s?.title || s?.firstPrompt || "（无标题）" }}</div>
        <div class="sub">
          <span :title="s?.project ?? ''">{{ s?.project }}</span>
          <n-tag v-if="s?.gitBranch" size="tiny" :bordered="false">{{ s.gitBranch }}</n-tag>
        </div>
        <div class="sub">
          <span>{{ fmt(s?.startedAt) }} → {{ fmt(s?.endedAt) }}</span>
          <span>{{ s?.promptCount }} 条提示</span>
          <span :title="exactNumber(s?.totalTokens ?? 0)">{{ fmtTokens(s?.totalTokens ?? 0) }} tokens</span>
          <span>估算 {{ fmtUsd(s?.costUsd ?? 0) }}{{ s?.priced ? "" : " *" }}</span>
        </div>
        <n-space align="center" :size="8" style="margin-top: 8px">
          <n-button size="small" @click="copyResume">复制 resume 命令</n-button>
          <n-button size="small" @click="openDir">打开项目目录</n-button>
          <span class="switch">
            <n-switch v-model:value="store.showMeta" size="small" />
            显示系统内容<template v-if="hiddenMeta">（{{ hiddenMeta }}）</template>
          </span>
        </n-space>
        <n-breadcrumb v-if="store.stack.length > 1" style="margin-top: 8px">
          <n-breadcrumb-item v-for="(d, i) in store.stack" :key="i" @click="store.popTo(i)">
            {{ i === 0 ? "主会话" : `子代理 ${d.agentId}` }}
          </n-breadcrumb-item>
        </n-breadcrumb>
      </div>

      <n-alert v-if="store.detailError" type="error" :show-icon="false" style="margin: 8px 0">{{ store.detailError }}</n-alert>

      <div ref="scroller" class="messages" @scroll.passive="onScroll" @click="blockLinks">
        <div v-if="store.detailLoading" class="loading"><n-spin size="small" /></div>
        <template v-else-if="store.current">
          <n-empty v-if="visibleItems.length === 0" description="没有可显示的内容" style="margin-top: 60px" />
          <MessageItem v-for="(item, i) in shown" :key="i" :item="item" @open-agent="store.openSubagent" />
          <div v-if="shown.length < visibleItems.length" class="loading">
            已显示 {{ shown.length }} / {{ visibleItems.length }} 条，继续向下滚动加载
          </div>
          <div v-if="store.current.badLines" class="bad">日志中有 {{ store.current.badLines }} 行无法解析，已跳过</div>
        </template>
      </div>
    </template>
  </div>
</template>

<style scoped>
.pane {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  min-width: 0;
}
.header {
  padding-bottom: 10px;
  border-bottom: 1px solid rgba(128, 128, 128, 0.2);
}
.title {
  font-size: 16px;
  font-weight: 600;
}
.sub {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 14px;
  align-items: center;
  font-size: 12px;
  opacity: 0.65;
  margin-top: 4px;
  font-variant-numeric: tabular-nums;
}
.switch {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  margin-left: 8px;
}
.messages {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  padding: 0 8px 24px 2px;
}
.loading {
  display: flex;
  justify-content: center;
  padding: 16px 0;
  font-size: 12px;
  opacity: 0.6;
}
.bad {
  font-size: 12px;
  opacity: 0.55;
  text-align: center;
  margin-top: 12px;
}
</style>
