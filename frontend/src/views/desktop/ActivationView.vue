<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import PageHeader from '@/components/common/PageHeader.vue'
import { userApi, type DeviceInfo, type DonationQrCode, type ActivationStatus } from '@/api/user'

const status = ref<ActivationStatus | null>(null)
const deviceInfo = ref<DeviceInfo | null>(null)
const qr = ref<DonationQrCode | null>(null)
const licenseInput = ref('')
const loading = ref(false)

async function refresh() {
  loading.value = true
  try {
    status.value = await userApi.checkActivation()
    deviceInfo.value = await userApi.getDeviceInfo()
    qr.value = await userApi.getDonationQrcode()
  } finally {
    loading.value = false
  }
}

async function activate() {
  if (!licenseInput.value.trim()) {
    ElMessage.warning('请输入许可证')
    return
  }
  try {
    await userApi.activate(licenseInput.value.trim())
    ElMessage.success('激活成功!')
    await refresh()
  } catch (e: any) {
    ElMessage.error(`激活失败: ${e?.message ?? e}`)
  }
}

async function devGenerate() {
  try {
    licenseInput.value = await userApi.devGenerateLicense()
    ElMessage.info('已生成测试许可证(仅开发可用),点击激活即可')
  } catch (e: any) {
    ElMessage.error(`生成失败: ${e?.message ?? e}`)
  }
}

onMounted(refresh)
</script>

<template>
  <div class="activation-view" v-loading="loading">
    <PageHeader title="捐赠激活" subtitle="1 元捐赠激活全部功能 · 无会员分级 · 无订阅费" />

    <div class="status-card app-card">
      <el-tag :type="status?.activated ? 'success' : 'warning'" size="large" effect="dark">
        {{ status?.activated ? '✓ 已激活' : '未激活' }}
      </el-tag>
      <span v-if="status?.plan" class="plan">套餐: {{ status.plan }}</span>
    </div>

    <div class="activate-grid">
      <div class="qr-card app-card">
        <h3>方式一:扫码捐赠</h3>
        <div class="qr-area">
          <img v-if="qr" :src="qr.qr_base64" alt="微信收款码" class="qr-img" />
        </div>
        <p class="qr-tip">{{ qr?.amount_label }}</p>
        <div class="device-id">
          <span class="label">设备指纹:</span>
          <code>{{ status?.device_id }}</code>
        </div>
        <p class="device-url" v-if="qr">激活链接: {{ qr.activate_url }}</p>
      </div>

      <div class="license-card app-card">
        <h3>方式二:输入许可证</h3>
        <p class="card-desc">完成捐赠后,将服务端返回的许可证粘贴到下方</p>
        <el-input
          v-model="licenseInput"
          type="textarea"
          :rows="6"
          placeholder="粘贴许可证(base64 字符串)"
        />
        <div class="action-bar" style="margin-top: 16px">
          <el-button type="primary" @click="activate">激活</el-button>
          <el-button @click="devGenerate" v-if="!status?.activated">
            生成测试许可证(开发用)
          </el-button>
        </div>
      </div>
    </div>

    <div class="device-card app-card" v-if="deviceInfo">
      <h3>设备信息</h3>
      <el-descriptions :column="2" border>
        <el-descriptions-item label="操作系统">{{ deviceInfo.os_name }}</el-descriptions-item>
        <el-descriptions-item label="系统版本">{{ deviceInfo.os_version }}</el-descriptions-item>
        <el-descriptions-item label="主机名">{{ deviceInfo.hostname }}</el-descriptions-item>
        <el-descriptions-item label="CPU">{{ deviceInfo.cpu_brand }}</el-descriptions-item>
        <el-descriptions-item label="架构">{{ deviceInfo.arch }}</el-descriptions-item>
        <el-descriptions-item label="设备指纹">
          <code>{{ deviceInfo.device_id }}</code>
        </el-descriptions-item>
      </el-descriptions>
    </div>
  </div>
</template>

<style scoped lang="scss">

.status-card {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 20px;
}
.plan {
  color: $text-regular;
}
.activate-grid {
  display: grid;
  grid-template-columns: 320px 1fr;
  gap: 20px;
  margin-bottom: 20px;
}
.qr-card,
.license-card {
  h3 {
    margin: 0 0 12px;
    font-size: 16px;
  }
}
.qr-area {
  text-align: center;
  padding: 20px;
  background: #fff;
  border: 1px dashed $border-color;
  border-radius: $radius-base;
  margin-bottom: 12px;
}
.qr-img {
  width: 180px;
  height: 180px;
  object-fit: contain;
}
.qr-tip {
  text-align: center;
  color: $police-blue;
  font-weight: 600;
  margin: 8px 0;
}
.device-id {
  font-size: 12px;
  color: $text-secondary;
  word-break: break-all;
  .label {
    margin-right: 4px;
  }
  code {
    font-family: monospace;
  }
}
.device-url {
  font-size: 11px;
  color: $text-secondary;
  word-break: break-all;
}
.card-desc {
  color: $text-secondary;
  font-size: 13px;
  margin: 0 0 12px;
}
.device-card {
  h3 {
    margin: 0 0 12px;
    font-size: 16px;
  }
}
</style>
