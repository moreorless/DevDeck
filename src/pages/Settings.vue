<script setup lang="ts">
import { NH2, NForm, NFormItem, NRadioGroup, NRadioButton } from "naive-ui";
import { useThemeStore } from "../app/theme";

const theme = useThemeStore();
</script>

<template>
  <n-h2>设置</n-h2>
  <n-form label-placement="left" label-width="96">
    <n-form-item label="主题">
      <n-radio-group v-model:value="theme.mode">
        <n-radio-button value="system">跟随系统</n-radio-button>
        <n-radio-button value="light">浅色</n-radio-button>
        <n-radio-button value="dark">深色</n-radio-button>
      </n-radio-group>
    </n-form-item>
  </n-form>
</template>
