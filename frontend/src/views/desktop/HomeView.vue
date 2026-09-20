<script setup lang="ts">
import { useRouter } from 'vue-router'
import PageHeader from '@/components/common/PageHeader.vue'
import { useActivationStore } from '@/stores/activation'

const router = useRouter()
const activation = useActivationStore()

const features = [
  { icon: '🔗', title: 'PDF 合并', desc: '多个 PDF 合并为一个', path: '/pdf-tools' },
  { icon: '✂️', title: 'PDF 拆分', desc: '按页码范围或每 N 页拆分', path: '/pdf-tools' },
  { icon: '🔄', title: '页面操作', desc: '旋转/删除/提取/插入', path: '/pdf-tools' },
  { icon: '🔒', title: 'PDF 加密', desc: 'AES-256 密码保护', path: '/pdf-tools' },
  { icon: '💧', title: 'PDF 水印', desc: '文字/图片水印', path: '/pdf-tools' },
  { icon: '📦', title: 'PDF 压缩', desc: '无损压缩体积', path: '/pdf-tools' },
  { icon: '🖼️', title: '转图片', desc: 'PDF 转 PNG/JPG/WebP', path: '/pdf-tools' },
  { icon: '📁', title: '文档管理', desc: '本地文档分类管理', path: '/documents' },
]

const highlights = [
  { icon: '🛡️', title: '本地优先', desc: '所有处理在本地完成,文件不离开本机' },
  { icon: '⚡', title: '高性能', desc: 'Rust 核心,合并/拆分毫秒级完成' },
  { icon: '🔒', title: '隐私至上', desc: '零数据上传,完全离线处理' },
  { icon: '💡', title: '轻量安装', desc: 'Tauri 桌面端,安装包仅几十 MB' },
]
</script>

<template>
  <div class="home-view">
    <PageHeader title="PDF-DocVault 密档大师" subtitle="桌面端隐私文档处理中心 · 本地优先 · 隐私至上" />

    <div class="hero app-card">
      <div class="hero-text">
        <h2>一站式 PDF 文档处理工具</h2>
        <p>集成 PDF 转换、文档扫描、OCR 识别能力,所有任务在本地完成,数据不外传。</p>
        <div class="hero-actions">
          <el-button type="primary" size="large" @click="router.push('/pdf-tools')">
            开始使用
          </el-button>
          <el-button
            v-if="!activation.activated"
            size="large"
            @click="router.push('/activation')"
          >
            激活
          </el-button>
        </div>
      </div>
    </div>

    <div class="highlights">
      <div v-for="h in highlights" :key="h.title" class="highlight-card app-card">
        <span class="h-icon">{{ h.icon }}</span>
        <div>
          <div class="h-title">{{ h.title }}</div>
          <div class="h-desc">{{ h.desc }}</div>
        </div>
      </div>
    </div>

    <h3 class="section-title">功能</h3>
    <div class="feature-grid">
      <div
        v-for="f in features"
        :key="f.title"
        class="feature-card app-card"
        @click="router.push(f.path)"
      >
        <span class="f-icon">{{ f.icon }}</span>
        <div class="f-title">{{ f.title }}</div>
        <div class="f-desc">{{ f.desc }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">

.hero {
  background: linear-gradient(135deg, $police-blue 0%, $police-blue-dark 100%);
  color: #fff;
  margin-bottom: 24px;
  h2 {
    margin: 0 0 8px;
    font-size: 24px;
  }
  p {
    margin: 0 0 16px;
    opacity: 0.9;
  }
}
.hero-actions {
  display: flex;
  gap: 12px;
}
.highlights {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 16px;
  margin-bottom: 28px;
}
.highlight-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px;
}
.h-icon {
  font-size: 28px;
}
.h-title {
  font-weight: 600;
  font-size: 14px;
}
.h-desc {
  font-size: 12px;
  color: $text-secondary;
}
.section-title {
  font-size: 18px;
  margin: 0 0 16px;
}
.feature-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 16px;
}
.feature-card {
  cursor: pointer;
  transition: all 0.2s;
  text-align: center;
  padding: 24px 16px;
  &:hover {
    transform: translateY(-2px);
    box-shadow: 0 6px 16px rgba(30, 90, 168, 0.15);
    border: 1px solid $police-blue;
  }
}
.f-icon {
  font-size: 36px;
}
.f-title {
  font-weight: 600;
  margin: 10px 0 4px;
}
.f-desc {
  font-size: 12px;
  color: $text-secondary;
}
</style>
