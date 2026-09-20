<script setup lang="ts">
import { useActivationStore } from '@/stores/activation'
import { onMounted } from 'vue'

const activation = useActivationStore()
onMounted(() => {
  activation.check()
})
</script>

<template>
  <header class="app-header">
    <div class="brand">
      <span class="logo">📄</span>
      <div class="brand-text">
        <span class="title">PDF-DocVault</span>
        <span class="subtitle">密档大师 · 本地优先 · 隐私至上</span>
      </div>
    </div>
    <div class="header-right">
      <el-tag :type="activation.activated ? 'success' : 'warning'" effect="light" size="small">
        {{ activation.activated ? '已激活' : '未激活' }}
      </el-tag>
    </div>
  </header>
</template>

<style scoped lang="scss">

.app-header {
  height: $header-height;
  background: linear-gradient(90deg, $police-blue 0%, $police-blue-dark 100%);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 20px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
}
.logo {
  font-size: 24px;
}
.brand-text {
  display: flex;
  flex-direction: column;
  line-height: 1.2;
}
.title {
  font-size: 16px;
  font-weight: 600;
}
.subtitle {
  font-size: 11px;
  opacity: 0.85;
}
</style>
