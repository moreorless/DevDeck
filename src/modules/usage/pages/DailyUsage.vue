<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { NH2, NAlert, NEmpty, NButton, NProgress, NSpace, NSpin, useMessage } from "naive-ui";
import { useUsageStore } from "../store";
import { useThemeStore } from "../../../app/theme";
import KpiCards from "../components/KpiCards.vue";
import FilterBar from "../components/FilterBar.vue";
import UsageChart from "../components/UsageChart.vue";
import UsageTable from "../components/UsageTable.vue";
import QuotaCard from "../../quota/components/QuotaCard.vue";
import { useSettingsStore } from "../../../app/settings";

const store = useUsageStore();
const theme = useThemeStore();
const router = useRouter();
const message = useMessage();
const exporting = ref(false);
const settings = useSettingsStore();
const quotaOn = computed(() => settings.config?.quota.enabled ?? true);

onMounted(() => store.init());

const noLogDir = computed(() => store.status !== null && store.status.foundDirs.length === 0);
/** 首次扫描尚未完成 */
const firstScan = computed(() => store.status?.scanning && !store.status.lastScan?.finishedAt);

async function onExport() {
  exporting.value = true;
  try {
    const r = await store.exportToCsv();
    if (r) message.success(`已导出 ${r.rows} 行到 ${r.path}`);
  } catch (e) {
    message.error(String(e));
  } finally {
    exporting.value = false;
  }
}
</script>

<template>
  <n-h2 style="margin-bottom: 12px">每日用量</n-h2>

  <n-empty v-if="noLogDir" description="未找到 Claude Code 日志目录" style="margin-top: 80px">
    <template #extra>
      <div class="hint">已检查：{{ store.status?.checkedDirs.join("；") }}</div>
      <n-button size="small" style="margin-top: 12px" @click="router.push('/settings')">去设置</n-button>
    </template>
  </n-empty>

  <template v-else>
    <n-alert v-if="store.scanError" type="warning" closable style="margin-bottom: 12px" @close="store.scanError = null">
      刷新失败，当前显示的是上次的数据：{{ store.scanError }}
    </n-alert>
    <n-alert v-if="store.loadError" type="error" style="margin-bottom: 12px">读取数据失败：{{ store.loadError }}</n-alert>

    <div v-if="firstScan" class="first-scan">
      <n-spin size="small" />
      <span>首次扫描日志中，完成后自动显示数据</span>
      <n-progress
        v-if="store.progress"
        type="line"
        :percentage="Math.round((store.progress.done / store.progress.total) * 100)"
        style="width: 320px"
      />
    </div>

    <n-space vertical :size="12">
      <QuotaCard v-if="quotaOn" />
      <KpiCards :summary="store.summary" />
      <FilterBar />
      <UsageChart
        v-model:stack-by="store.stackBy"
        :rows="store.rows"
        :since="store.range[0]"
        :until="store.range[1]"
        :model-order="store.modelOrder"
        :dark="theme.isDark"
      />
      <UsageTable :rows="store.rows" :exporting="exporting" @export="onExport" />
    </n-space>
  </template>
</template>

<style scoped>
.hint {
  font-size: 12px;
  opacity: 0.7;
}
.first-scan {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
}
</style>
