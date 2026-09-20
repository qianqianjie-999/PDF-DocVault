<script setup lang="ts">
import { useRouter } from 'vue-router'

const router = useRouter()

const menus = [
  { path: '/home', label: '首页', icon: '🏠' },
  { path: '/pdf-tools', label: 'PDF 工具', icon: '🛠️' },
  { path: '/scan', label: '文档扫描', icon: '📷' },
  { path: '/idcard', label: '证件排版', icon: '🪪' },
  { path: '/ocr', label: 'OCR 识别', icon: '🔍' },
  { path: '/word2pdf', label: 'Word 转 PDF', icon: '📄' },
  { path: '/images2pdf', label: '图片转 PDF', icon: '🖼️' },
  { path: '/documents', label: '文档管理', icon: '📁' },
  { path: '/activation', label: '激活', icon: '🔑' },
  { path: '/settings', label: '设置', icon: '⚙️' },
]
</script>

<template>
  <aside class="app-sidebar">
    <nav>
      <a
        v-for="m in menus"
        :key="m.path"
        class="menu-item"
        :class="{ active: router.currentRoute.value.path === m.path }"
        @click="router.push(m.path)"
      >
        <span class="icon">{{ m.icon }}</span>
        <span class="label">{{ m.label }}</span>
      </a>
    </nav>
    <div class="sidebar-footer">
      <span class="version">v0.1.0</span>
    </div>
  </aside>
</template>

<style scoped lang="scss">

.app-sidebar {
  width: $sidebar-width;
  background: #fff;
  border-right: 1px solid $border-color;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 12px 0;
}
nav {
  display: flex;
  flex-direction: column;
}
.menu-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 20px;
  cursor: pointer;
  color: $text-regular;
  border-left: 3px solid transparent;
  transition: all 0.2s;
  &:hover {
    background: $police-blue-50;
    color: $police-blue;
  }
  &.active {
    background: $police-blue-50;
    color: $police-blue;
    border-left-color: $police-blue;
    font-weight: 600;
  }
}
.icon {
  font-size: 18px;
}
.label {
  font-size: 14px;
}
.sidebar-footer {
  padding: 12px 20px;
  font-size: 12px;
  color: $text-secondary;
}
</style>
