<script setup lang="ts">
import { ElMessage } from 'element-plus'
import { open } from '@tauri-apps/plugin-dialog'
import PageHeader from '@/components/common/PageHeader.vue'
import { useSettingsStore } from '@/stores/settings'

const settings = useSettingsStore()

// App.vue 启动时已加载持久化设置;此处不再重复 load,
// 否则未点保存就离开再回来,编辑内容会被旧值悄悄覆盖

async function chooseOutputDir() {
  const dir = await open({
    directory: true,
    title: '选择默认输出目录',
    defaultPath: settings.defaultOutputDir || undefined,
  })
  if (dir) settings.defaultOutputDir = dir
}

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
          <el-input
            v-model="settings.defaultOutputDir"
            placeholder="留空则输出到源文件旁"
            class="dir-input"
          >
            <template #append>
              <el-button @click="chooseOutputDir">选择</el-button>
            </template>
          </el-input>
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
        <el-form-item label="OCR 识别语言">
          <el-select v-model="settings.ocrLanguage">
            <el-option label="中文+英文" value="ch" />
            <el-option label="英文" value="en" />
          </el-select>
          <span class="hint">默认引擎为中英文混合识别;英文选项在 paddleocr 引擎下加载英文专用模型</span>
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
.dir-input {
  max-width: 480px;
}
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
