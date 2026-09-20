<script setup lang="ts">
import { onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import PageHeader from '@/components/common/PageHeader.vue'
import { useSettingsStore } from '@/stores/settings'

const settings = useSettingsStore()

onMounted(() => settings.load())

function save() {
  settings.save()
  ElMessage.success('设置已保存')
}
</script>

<template>
  <div class="settings-view">
    <PageHeader title="设置" subtitle="配置默认输出与处理参数" />

    <div class="app-card">
      <el-form label-width="140px" label-position="left">
        <el-form-item label="默认输出目录">
          <el-input v-model="settings.defaultOutputDir" placeholder="留空则使用源文件目录" />
        </el-form-item>
        <el-form-item label="默认 DPI">
          <el-input-number v-model="settings.defaultDpi" :min="72" :max="600" :step="72" />
        </el-form-item>
        <el-form-item label="默认图片格式">
          <el-radio-group v-model="settings.defaultImageFormat">
            <el-radio value="png">PNG</el-radio>
            <el-radio value="jpg">JPG</el-radio>
            <el-radio value="webp">WebP</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="OCR 语言(预留)">
          <el-select v-model="settings.ocrLanguage" disabled>
            <el-option label="中文+英文" value="ch" />
            <el-option label="英文" value="en" />
          </el-select>
          <span class="hint">OCR 功能将在后续版本提供</span>
        </el-form-item>
        <el-form-item label="完成后自动打开">
          <el-switch v-model="settings.autoOpenOutput" />
          <span class="hint">功能完成后自动打开成果文件或所在文件夹</span>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="save">保存设置</el-button>
        </el-form-item>
      </el-form>
    </div>

    <div class="app-card about">
      <h3>关于</h3>
      <p>PDF-DocVault 密档大师 v0.1.0</p>
      <p class="muted">本地优先 · 隐私至上 · 桌面端隐私文档处理中心</p>
      <p class="muted">所有文档处理任务在本地完成,文件数据永不离开本机。</p>
    </div>
  </div>
</template>

<style scoped lang="scss">
.hint {
  margin-left: 12px;
  font-size: 12px;
  color: #909399;
}
.about {
  margin-top: 20px;
  h3 {
    margin: 0 0 8px;
  }
  p {
    margin: 4px 0;
  }
  .muted {
    color: #909399;
    font-size: 13px;
  }
}
</style>
