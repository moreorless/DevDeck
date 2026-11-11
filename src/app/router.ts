import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

/** 每个路由在侧边栏中的展示信息 */
export interface NavMeta {
  title: string;
  /** 所属分组：日常开发 / 个人任务 / 系统 */
  group: "dev" | "personal" | "system";
}

export const routes: RouteRecordRaw[] = [
  { path: "/", redirect: "/usage" },
  {
    path: "/usage",
    component: () => import("../modules/usage/pages/DailyUsage.vue"),
    meta: { title: "每日用量", group: "dev" } satisfies NavMeta,
  },
  {
    path: "/sessions",
    component: () => import("../modules/sessions/pages/SessionList.vue"),
    meta: { title: "会话浏览", group: "dev" } satisfies NavMeta,
  },
  {
    path: "/settings",
    component: () => import("../pages/Settings.vue"),
    meta: { title: "设置", group: "system" } satisfies NavMeta,
  },
  {
    path: "/about",
    component: () => import("../pages/About.vue"),
    meta: { title: "关于", group: "system" } satisfies NavMeta,
  },
];

// Tauri 本地页面没有服务端路由，用 hash 模式最稳妥
export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
