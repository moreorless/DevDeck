<script setup lang="ts">
import { computed } from "vue";
import { NInput, NSelect, NDatePicker, NButton, NEmpty, NSpin, NText, NAlert } from "naive-ui";
import { useSessionsStore } from "../store";
import { parseYmd, ymd } from "../../../shared/date";
import { fmtTokens, fmtUsd } from "../../../shared/format";

const store = useSessionsStore();

let timer: number | undefined;
function onKeyword(v: string) {
  store.keyword = v;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => store.loadList(), 300);
}

const projectOptions = computed(() => store.projects.map((p) => ({ label: p, value: p })));

const pickerValue = computed<[number, number] | null>(() =>
  store.range ? [parseYmd(store.range[0]).getTime(), parseYmd(store.range[1]).getTime()] : null,
);
function onPick(v: [number, number] | null) {
  store.range = v ? [ymd(new Date(v[0])), ymd(new Date(v[1]))] : null;
  store.loadList();
}

function fmtWhen(s: string | null) {
  if (!s) return "-";
  const d = new Date(s);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 项目路径只显示最后一段 */
const shortProject = (p: string | null) => (p ? p.split(/[\\/]/).filter(Boolean).pop() ?? p : "-");
</script>

<template>
  <div class="pane">
    <div class="filters">
      <n-input :value="store.keyword" size="small" clearable placeholder="搜索标题或提示" @update:value="onKeyword" />
      <n-select
        :value="store.project"
        :options="projectOptions"
        size="small"
        clearable
        filterable
        placeholder="全部项目"
        @update:value="(v: string | null) => ((store.project = v), store.loadList())"
      />
      <n-date-picker type="daterange" size="small" clearable :value="pickerValue" @update:value="onPick" />
      <n-text depth="3" class="count">共 {{ store.total.toLocaleString() }} 个会话</n-text>
    </div>

    <n-alert v-if="store.listError" type="error" :show-icon="false" class="err">{{ store.listError }}</n-alert>

    <div class="list">
      <n-empty v-if="!store.listLoading && store.items.length === 0" description="没有匹配的会话" style="margin-top: 60px" />
      <div
        v-for="s in store.items"
        :key="s.sessionId"
        class="item"
        :class="{ active: s.sessionId === store.selectedId }"
        @click="store.select(s.sessionId)"
      >
        <div class="title">{{ s.title || s.firstPrompt || "（无标题）" }}</div>
        <div v-if="s.title && s.firstPrompt" class="prompt">{{ s.firstPrompt }}</div>
        <div class="meta">
          <span :title="s.project ?? ''">{{ shortProject(s.project) }}</span>
          <span>{{ fmtWhen(s.endedAt) }}</span>
        </div>
        <div class="meta">
          <span>{{ s.promptCount }} 条提示</span>
          <span>{{ fmtTokens(s.totalTokens) }} tokens · {{ fmtUsd(s.costUsd) }}{{ s.priced ? "" : " *" }}</span>
        </div>
      </div>
      <div class="more">
        <n-spin v-if="store.listLoading" size="small" />
        <n-button v-else-if="store.hasMore" size="small" quaternary @click="store.loadList(false)">加载更多</n-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pane {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.filters {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-bottom: 8px;
}
.count {
  font-size: 12px;
}
.err {
  margin-bottom: 8px;
}
.list {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  margin-right: -4px;
  padding-right: 4px;
}
.item {
  padding: 10px 10px;
  border-radius: 6px;
  cursor: pointer;
  border: 1px solid transparent;
}
.item:hover {
  background: rgba(128, 128, 128, 0.08);
}
.item.active {
  background: rgba(24, 160, 88, 0.1);
  border-color: rgba(24, 160, 88, 0.35);
}
.title {
  font-weight: 600;
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.prompt {
  font-size: 12px;
  opacity: 0.75;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 2px;
}
.meta {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  opacity: 0.6;
  margin-top: 3px;
  font-variant-numeric: tabular-nums;
}
.meta span:first-child {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.more {
  display: flex;
  justify-content: center;
  padding: 8px 0 4px;
}
</style>
