<script setup lang="ts">
import { computed, h, onMounted, ref } from "vue";
import {
  NDataTable,
  NButton,
  NSpace,
  NTag,
  NModal,
  NForm,
  NFormItem,
  NInputNumber,
  NText,
  NSwitch,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import { getPricing, useSettingsStore, type PriceRow, type PricingView, type Rates } from "../../app/settings";

const settings = useSettingsStore();
const message = useMessage();
const pricing = ref<PricingView | null>(null);
const onlyInUse = ref(true);

async function load() {
  try {
    pricing.value = await getPricing();
  } catch (e) {
    message.error(String(e));
  }
}
onMounted(load);

const rows = computed(() => (pricing.value?.rows ?? []).filter((r) => !onlyInUse.value || r.inUse));

const fmt = (v: number | undefined) => (v === undefined ? "-" : `$${+v.toFixed(4)}`);
const effective = (r: PriceRow) => r.override ?? r.builtin;

function rateCol(title: string, key: keyof Rates) {
  return {
    title,
    key,
    align: "right" as const,
    render: (r: PriceRow) => fmt(effective(r)?.[key]),
  };
}

const columns: DataTableColumns<PriceRow> = [
  {
    title: "模型",
    key: "model",
    render: (r) =>
      h(NSpace, { size: 4, align: "center", wrap: false }, () => [
        h("span", r.model),
        r.inUse ? h(NTag, { size: "tiny", bordered: false }, () => "使用中") : null,
      ]),
  },
  {
    title: "来源",
    key: "source",
    width: 90,
    render: (r) =>
      r.override
        ? h(NTag, { size: "small", type: "warning", bordered: false }, () => "自定义")
        : r.builtin
          ? h(NText, { depth: 3 }, () => (r.longContext ? "内置 · 分档" : "内置"))
          : h(NTag, { size: "small", type: "error", bordered: false }, () => "价格未知"),
  },
  rateCol("Input", "input"),
  rateCol("Output", "output"),
  rateCol("Cache 写 5m", "cacheWrite5m"),
  rateCol("Cache 写 1h", "cacheWrite1h"),
  rateCol("Cache 读", "cacheRead"),
  {
    title: "",
    key: "actions",
    width: 150,
    render: (r) =>
      h(NSpace, { size: 4, wrap: false }, () => [
        h(NButton, { size: "tiny", onClick: () => openEdit(r) }, () => "编辑"),
        r.override ? h(NButton, { size: "tiny", quaternary: true, onClick: () => reset(r) }, () => "恢复内置") : null,
      ]),
  },
];

// 编辑弹窗
const editing = ref<PriceRow | null>(null);
const form = ref<Rates>({ input: 0, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0, output: 0 });

function openEdit(r: PriceRow) {
  editing.value = r;
  form.value = { ...(effective(r) ?? { input: 0, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0, output: 0 }) };
}

async function saveOverrides(next: Record<string, Rates>) {
  try {
    await settings.save({ pricingOverrides: next });
    await load();
    message.success("价格已更新，费用将按新价格重新计算");
  } catch (e) {
    message.error(String(e));
  }
}

async function submit() {
  if (!editing.value) return;
  const next = { ...(settings.config?.pricingOverrides ?? {}), [editing.value.model]: { ...form.value } };
  editing.value = null;
  await saveOverrides(next);
}

async function reset(r: PriceRow) {
  const next = { ...(settings.config?.pricingOverrides ?? {}) };
  delete next[r.model];
  await saveOverrides(next);
}

const fields: { key: keyof Rates; label: string }[] = [
  { key: "input", label: "Input" },
  { key: "output", label: "Output" },
  { key: "cacheWrite5m", label: "Cache 写（5 分钟）" },
  { key: "cacheWrite1h", label: "Cache 写（1 小时）" },
  { key: "cacheRead", label: "Cache 读" },
];
</script>

<template>
  <n-space vertical :size="8">
    <n-space align="center" justify="space-between">
      <n-text depth="3" style="font-size: 12px">
        单位：美元 / 百万 tokens。内置价格来自 {{ pricing?.source }}，核对日期 {{ pricing?.checkedAt }}。
        分档计价的模型（如 Haiku 5.5）自定义后统一按一档计价。
      </n-text>
      <span style="font-size: 12px; white-space: nowrap">
        <n-switch v-model:value="onlyInUse" size="small" /> 只看本机用到的模型
      </span>
    </n-space>
    <n-data-table :columns="columns" :data="rows" :row-key="(r: PriceRow) => r.model" size="small" :max-height="360" />
  </n-space>

  <n-modal
    :show="!!editing"
    preset="dialog"
    :title="`自定义价格：${editing?.model}`"
    positive-text="保存"
    negative-text="取消"
    @positive-click="submit"
    @negative-click="editing = null"
    @close="editing = null"
  >
    <n-form label-placement="left" label-width="140" style="margin-top: 12px">
      <n-form-item v-for="f in fields" :key="f.key" :label="f.label">
        <n-input-number v-model:value="form[f.key]" :min="0" :step="0.01" :precision="4" style="width: 180px">
          <template #prefix>$</template>
        </n-input-number>
      </n-form-item>
    </n-form>
  </n-modal>
</template>
