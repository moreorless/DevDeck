<script setup lang="ts">
import { computed } from "vue";
import { NTag, NButton } from "naive-ui";
import type { Clipped, Item } from "../api";
import { renderMarkdown } from "../../../shared/markdown";
import { exactNumber } from "../../../shared/format";

const props = defineProps<{ item: Item }>();
const emit = defineEmits<{ openAgent: [agentId: string] }>();

const time = computed(() => {
  if (!props.item.ts) return "";
  const d = new Date(props.item.ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
});

const html = computed(() => (props.item.kind === "assistant" ? renderMarkdown(props.item.text) : ""));

const clipped = (c: Clipped) => c.fullLen > c.text.length;
const clipNote = (c: Clipped) => `（已截断，共 ${exactNumber(c.fullLen)} 字符，显示前 ${exactNumber(c.text.length)}）`;

/** 工具调用的一行摘要：取输入里最能说明意图的字段 */
const toolSummary = computed(() => {
  if (props.item.kind !== "toolUse") return "";
  try {
    const input = JSON.parse(props.item.input.text) as Record<string, unknown>;
    for (const k of ["description", "command", "file_path", "path", "pattern", "url", "query", "prompt", "skill"]) {
      const v = input[k];
      if (typeof v === "string" && v.trim()) {
        const line = v.trim().split("\n")[0];
        return line.length > 100 ? `${line.slice(0, 100)}…` : line;
      }
    }
  } catch {
    /* 输入被截断时不是合法 JSON */
  }
  return "";
});
</script>

<template>
  <!-- 用户输入 -->
  <div v-if="item.kind === 'user'" class="msg user">
    <div class="head"><span class="who">用户</span><span class="time">{{ time }}</span></div>
    <div class="bubble plain">{{ item.text }}</div>
    <n-tag v-if="item.images" size="small" :bordered="false" class="img-tag">附带 {{ item.images }} 张图片</n-tag>
  </div>

  <!-- 助手回复 -->
  <div v-else-if="item.kind === 'assistant'" class="msg">
    <div class="head">
      <span class="who">助手</span><span v-if="item.model" class="model">{{ item.model }}</span><span class="time">{{ time }}</span>
    </div>
    <!-- markdown-it 已关闭原始 HTML，渲染结果中不会出现会话内容注入的标签 -->
    <div class="md" v-html="html" />
  </div>

  <!-- 思考 -->
  <details v-else-if="item.kind === 'thinking'" class="fold">
    <summary><span class="fold-label">思考</span><span class="time">{{ time }}</span></summary>
    <div class="plain muted">{{ item.text.text }}</div>
    <div v-if="clipped(item.text)" class="note">{{ clipNote(item.text) }}</div>
  </details>

  <!-- 工具调用 -->
  <details v-else-if="item.kind === 'toolUse'" class="fold tool">
    <summary>
      <span class="fold-label">{{ item.name }}</span>
      <span class="summary">{{ toolSummary }}</span>
      <n-tag v-if="item.result?.isError" size="tiny" type="error" :bordered="false">出错</n-tag>
      <n-tag v-else-if="!item.result" size="tiny" :bordered="false">无结果</n-tag>
      <span class="time">{{ time }}</span>
    </summary>
    <div class="section-title">输入</div>
    <pre class="code">{{ item.input.text }}</pre>
    <div v-if="clipped(item.input)" class="note">{{ clipNote(item.input) }}</div>
    <template v-if="item.result">
      <div class="section-title">
        结果<span v-if="item.result.images">（含 {{ item.result.images }} 张图片，未显示）</span>
      </div>
      <pre class="code" :class="{ error: item.result.isError }">{{ item.result.content.text || "（空）" }}</pre>
      <div v-if="clipped(item.result.content)" class="note">{{ clipNote(item.result.content) }}</div>
    </template>
    <n-button
      v-if="item.agentId"
      size="tiny"
      secondary
      :disabled="!item.agentAvailable"
      style="margin-top: 6px"
      @click="emit('openAgent', item.agentId)"
    >
      {{ item.agentAvailable ? "查看子代理对话" : "子代理日志不存在" }}
    </n-button>
  </details>

  <!-- 压缩点 -->
  <div v-else-if="item.kind === 'compact'" class="compact">
    <div class="divider">
      <span>
        上下文已压缩{{ item.trigger === "auto" ? "（自动）" : item.trigger === "manual" ? "（手动）" : "" }}
        <template v-if="item.preTokens">· 压缩前 {{ exactNumber(item.preTokens) }} tokens</template>
      </span>
    </div>
    <details v-if="item.summary" class="fold">
      <summary><span class="fold-label">压缩摘要</span></summary>
      <div class="plain muted">{{ item.summary.text }}</div>
      <div v-if="clipped(item.summary)" class="note">{{ clipNote(item.summary) }}</div>
    </details>
  </div>

  <!-- 系统注入内容 -->
  <details v-else-if="item.kind === 'meta'" class="fold meta">
    <summary><span class="fold-label">{{ item.label }}</span><span class="time">{{ time }}</span></summary>
    <pre class="code">{{ item.text.text }}</pre>
    <div v-if="clipped(item.text)" class="note">{{ clipNote(item.text) }}</div>
  </details>
</template>

<style scoped>
.msg {
  margin: 14px 0;
}
.head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 4px;
  font-size: 12px;
}
.who {
  font-weight: 600;
  font-size: 13px;
}
.model,
.time {
  opacity: 0.55;
  font-variant-numeric: tabular-nums;
}
.time {
  margin-left: auto;
}
.user .bubble {
  background: rgba(42, 120, 214, 0.08);
  border: 1px solid rgba(42, 120, 214, 0.18);
  border-radius: 8px;
  padding: 8px 12px;
}
.plain {
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.6;
}
.muted {
  opacity: 0.75;
  font-size: 13px;
}
.img-tag {
  margin-top: 4px;
}
.fold {
  margin: 6px 0;
  border: 1px solid rgba(128, 128, 128, 0.2);
  border-radius: 6px;
  padding: 4px 10px;
}
.fold[open] {
  padding-bottom: 8px;
}
.fold summary {
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  list-style: none;
  min-height: 24px;
}
.fold summary::before {
  content: "▸";
  opacity: 0.5;
}
.fold[open] summary::before {
  content: "▾";
}
.fold-label {
  font-weight: 600;
  white-space: nowrap;
}
.summary {
  opacity: 0.65;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  font-family: Consolas, "Cascadia Mono", monospace;
}
.meta {
  opacity: 0.7;
  border-style: dashed;
}
.section-title {
  font-size: 12px;
  opacity: 0.6;
  margin: 8px 0 4px;
}
.code {
  margin: 0;
  padding: 8px 10px;
  border-radius: 6px;
  background: rgba(128, 128, 128, 0.08);
  font-family: Consolas, "Cascadia Mono", monospace;
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 420px;
  overflow: auto;
}
.code.error {
  background: rgba(208, 59, 59, 0.08);
}
.note {
  font-size: 12px;
  opacity: 0.55;
  margin-top: 4px;
}
.compact {
  margin: 18px 0;
}
.divider {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  opacity: 0.65;
}
.divider::before,
.divider::after {
  content: "";
  flex: 1;
  border-top: 1px dashed rgba(128, 128, 128, 0.5);
}
</style>
