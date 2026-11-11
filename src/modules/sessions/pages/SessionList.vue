<script setup lang="ts">
import { onMounted } from "vue";
import { useSessionsStore } from "../store";
import SessionListPane from "../components/SessionListPane.vue";
import SessionDetailPane from "../components/SessionDetailPane.vue";

const store = useSessionsStore();
onMounted(() => store.init());
</script>

<template>
  <div class="layout">
    <aside class="left"><SessionListPane /></aside>
    <section class="right"><SessionDetailPane /></section>
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
  width: 340px;
  flex-shrink: 0;
  min-height: 0;
  border-right: 1px solid rgba(128, 128, 128, 0.2);
  padding-right: 12px;
}
.right {
  flex: 1;
  min-width: 0;
  min-height: 0;
}
</style>
