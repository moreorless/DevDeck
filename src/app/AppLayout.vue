<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { NLayout, NLayoutSider, NLayoutContent, NMenu, type MenuOption } from "naive-ui";
import { routes, type NavMeta } from "./router";
import { useSettingsStore } from "./settings";

const route = useRoute();
const router = useRouter();
const settings = useSettingsStore();

/** 可开关的模块：配置中开启才出现在侧边栏 */
const featureOn = (f: NavMeta["feature"]) => !f || (f === "insights" && !!settings.config?.insights.enabled);

const GROUP_LABELS: Record<NavMeta["group"], string> = {
  dev: "日常开发",
  personal: "个人任务",
  system: "系统",
};

// 按分组从路由表生成侧边栏菜单；新增模块只需注册路由
const menuOptions = computed<MenuOption[]>(() => {
  const groups = new Map<NavMeta["group"], MenuOption[]>();
  for (const r of routes) {
    const meta = r.meta as NavMeta | undefined;
    if (!meta || !featureOn(meta.feature)) continue;
    const list = groups.get(meta.group) ?? [];
    list.push({ label: meta.title, key: r.path });
    groups.set(meta.group, list);
  }
  return [...groups.entries()].map(([g, children]) => ({
    type: "group",
    label: GROUP_LABELS[g],
    key: g,
    children,
  }));
});

const activeKey = computed(() => route.path);
</script>

<template>
  <n-layout has-sider class="root">
    <n-layout-sider bordered :width="180" content-style="padding: 12px 0;">
      <div class="brand">DevDeck</div>
      <n-menu
        :options="menuOptions"
        :value="activeKey"
        @update:value="(k: string) => router.push(k)"
      />
    </n-layout-sider>
    <n-layout-content content-style="padding: 20px 24px;">
      <router-view />
    </n-layout-content>
  </n-layout>
</template>

<style scoped>
.root {
  height: 100vh;
}
.brand {
  font-size: 18px;
  font-weight: 600;
  padding: 4px 20px 12px;
}
</style>
