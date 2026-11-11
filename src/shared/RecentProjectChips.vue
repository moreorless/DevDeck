<script setup lang="ts">
import { NTag } from "naive-ui";
import { shortProject, type RecentProject } from "./projects";

/** 最近活跃项目的快捷标签：点一下选中 / 取消 */
defineProps<{ recent: RecentProject[]; selected: string[] }>();
const emit = defineEmits<{ toggle: [project: string] }>();

const tip = (r: RecentProject) =>
  `${r.project}\n最后活动：${new Date(r.lastActiveAt).toLocaleString()} · ${r.sessionCount} 个会话`;
</script>

<template>
  <div v-if="recent.length" class="chips">
    <span class="label">最近：</span>
    <n-tag
      v-for="r in recent"
      :key="r.project"
      size="small"
      checkable
      :checked="selected.includes(r.project)"
      :title="tip(r)"
      @update:checked="emit('toggle', r.project)"
    >
      {{ shortProject(r.project) }}
    </n-tag>
  </div>
</template>

<style scoped>
.chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}
.label {
  font-size: 12px;
  opacity: 0.6;
}
</style>
