<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  NH2,
  NCard,
  NSpace,
  NForm,
  NFormItem,
  NRadioGroup,
  NRadioButton,
  NSelect,
  NSwitch,
  NButton,
  NTag,
  NAlert,
  NText,
  useMessage,
  useDialog,
} from "naive-ui";
import { open } from "@tauri-apps/plugin-dialog";
import { checkClaudeDir, listTimezones, useSettingsStore, type AppConfig } from "../app/settings";
import { useThemeStore, type ThemeMode } from "../app/theme";
import { refresh } from "../shared/data";
import PricingSection from "./settings/PricingSection.vue";
import InsightsSection from "./settings/InsightsSection.vue";

const settings = useSettingsStore();
const theme = useThemeStore();
const message = useMessage();
const dialog = useDialog();
const timezones = ref<string[]>([]);
const rebuilding = ref(false);

onMounted(async () => {
  try {
    await settings.load();
    timezones.value = await listTimezones();
  } catch (e) {
    message.error(String(e));
  }
});

const cfg = computed(() => settings.config);

async function save(patch: Partial<AppConfig>, ok?: string) {
  try {
    await settings.save(patch);
    if (ok) message.success(ok);
  } catch (e) {
    message.error(String(e));
    await settings.load(); // 回到已保存的状态
  }
}

/** 按当前系统的写法拼出主目录下的路径 */
function homeDir(...parts: string[]) {
  const sep = settings.view?.platform === "windows" ? "\\" : "/";
  return [settings.view?.homeLabel ?? "~", ...parts].join(sep);
}

// 日志目录
async function addDir() {
  const picked = await open({ directory: true, title: "选择 Claude 配置目录（包含 projects 子目录）" });
  if (typeof picked !== "string") return;
  try {
    if (!(await checkClaudeDir(picked))) {
      message.warning("该目录下没有 projects 子目录，不像 Claude Code 配置目录");
      return;
    }
  } catch (e) {
    message.error(String(e));
    return;
  }
  if (cfg.value?.claudeDirs.includes(picked) || settings.view?.dirs.some((d) => d.path === picked)) {
    message.info("该目录已在列表中");
    return;
  }
  await save({ claudeDirs: [...(cfg.value?.claudeDirs ?? []), picked] }, "已添加，正在扫描新目录");
}

async function removeDir(path: string) {
  await save({ claudeDirs: (cfg.value?.claudeDirs ?? []).filter((d) => d !== path) }, "已移除");
}

// 时区
const tzOptions = computed(() => [
  { label: `跟随系统（UTC${settings.view?.systemTimezone ?? ""}）`, value: "__system__" },
  ...timezones.value.map((t) => ({ label: t, value: t })),
]);
function onTimezone(v: string) {
  const tz = v === "__system__" ? null : v;
  if (tz === cfg.value?.timezone) return;
  dialog.warning({
    title: "修改时区",
    content: "按新时区重新划分每日用量，需要全量重建缓存（后台进行，期间可正常查看）。确定修改？",
    positiveText: "修改并重建",
    negativeText: "取消",
    onPositiveClick: () => save({ timezone: tz }, "时区已修改，正在重建缓存"),
  });
}

const refreshOptions = [
  { label: "1 分钟", value: 1 },
  { label: "5 分钟", value: 5 },
  { label: "15 分钟", value: 15 },
  { label: "30 分钟", value: 30 },
  { label: "关闭", value: 0 },
];

function onTheme(v: ThemeMode) {
  theme.mode = v;
  save({ theme: v });
}

function confirmRebuild() {
  dialog.warning({
    title: "全量重建缓存",
    content: "将重新读取全部日志（已被 Claude Code 清理的历史用量会保留），期间旧数据仍可查看。确定继续？",
    positiveText: "重建",
    negativeText: "取消",
    onPositiveClick: async () => {
      rebuilding.value = true;
      try {
        const s = await refresh("full");
        message.success(`重建完成：${s.filesTotal} 个文件，耗时 ${(s.durationMs / 1000).toFixed(1)} 秒`);
      } catch (e) {
        message.error(String(e));
      } finally {
        rebuilding.value = false;
      }
    },
  });
}
</script>

<template>
  <n-h2>设置</n-h2>
  <n-alert v-if="settings.view?.configError" type="warning" title="配置文件无法读取，当前使用默认配置" style="margin-bottom: 12px">
    {{ settings.view.configError }}。修改任一设置后会覆盖为新的配置文件。
  </n-alert>

  <n-space v-if="cfg" vertical :size="12" style="max-width: 980px">
    <n-card title="数据源" size="small">
      <n-space vertical :size="6">
        <div v-for="d in settings.view?.dirs" :key="d.path" class="dir">
          <span class="path">{{ d.path }}</span>
          <n-tag size="small" :type="d.exists ? 'success' : 'default'" :bordered="false">
            {{ d.exists ? "已找到" : "不存在" }}
          </n-tag>
          <n-text depth="3" style="font-size: 12px">{{ d.custom ? "手动添加" : "自动检测" }}</n-text>
          <n-button v-if="d.custom" size="tiny" quaternary @click="removeDir(d.path)">移除</n-button>
        </div>
        <n-space align="center">
          <n-button size="small" @click="addDir">添加目录</n-button>
          <n-text depth="3" style="font-size: 12px">
            自动检测顺序：CLAUDE_CONFIG_DIR → {{ homeDir(".claude") }} → {{ homeDir(".config", "claude") }}
          </n-text>
        </n-space>
      </n-space>
    </n-card>

    <n-card title="统计与刷新" size="small">
      <n-form label-placement="left" label-width="110">
        <n-form-item label="时区">
          <n-select
            :value="cfg.timezone ?? '__system__'"
            :options="tzOptions"
            filterable
            style="width: 320px"
            @update:value="onTimezone"
          />
        </n-form-item>
        <n-form-item label="自动刷新">
          <n-radio-group
            :value="cfg.refreshIntervalMin"
            @update:value="(v: number) => save({ refreshIntervalMin: v }, v ? `每 ${v} 分钟自动刷新` : '已关闭自动刷新')"
          >
            <n-radio-button v-for="o in refreshOptions" :key="o.value" :value="o.value">{{ o.label }}</n-radio-button>
          </n-radio-group>
        </n-form-item>
        <n-form-item label="套餐额度">
          <n-switch
            :value="cfg.quota.enabled"
            @update:value="(v: boolean) => save({ quota: { enabled: v } }, v ? '已开启套餐额度显示' : '已关闭套餐额度显示')"
          />
          <n-text depth="3" style="font-size: 12px; margin-left: 12px">
            在「每日用量」页显示当前会话、本周与各模型的已用百分比；定期运行 claude -p /usage 读取，不调用模型、不消耗额度
          </n-text>
        </n-form-item>
        <n-form-item label="缓存">
          <n-button size="small" :loading="rebuilding" @click="confirmRebuild">全量重建缓存</n-button>
        </n-form-item>
      </n-form>
    </n-card>

    <n-card title="价格表" size="small">
      <PricingSection />
    </n-card>

    <n-card title="Insights" size="small">
      <InsightsSection />
    </n-card>

    <n-card title="系统与外观" size="small">
      <n-form label-placement="left" label-width="110">
        <n-form-item label="开机自启">
          <n-switch
            :value="cfg.autostart"
            @update:value="(v: boolean) => save({ autostart: v }, v ? '已开启开机自启（启动后驻留托盘）' : '已关闭开机自启')"
          />
        </n-form-item>
        <n-form-item label="关闭窗口时">
          <n-radio-group :value="cfg.closeToTray" @update:value="(v: boolean) => save({ closeToTray: v })">
            <n-radio-button :value="true">最小化到托盘</n-radio-button>
            <n-radio-button :value="false">退出程序</n-radio-button>
          </n-radio-group>
        </n-form-item>
        <n-form-item label="主题">
          <n-radio-group :value="theme.mode" @update:value="onTheme">
            <n-radio-button value="system">跟随系统</n-radio-button>
            <n-radio-button value="light">浅色</n-radio-button>
            <n-radio-button value="dark">深色</n-radio-button>
          </n-radio-group>
        </n-form-item>
      </n-form>
    </n-card>
  </n-space>
</template>

<style scoped>
.dir {
  display: flex;
  align-items: center;
  gap: 10px;
}
.path {
  font-family: Consolas, "Cascadia Mono", "SF Mono", Menlo, monospace;
  font-size: 13px;
}
</style>
