<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { open } from '@tauri-apps/plugin-shell'
import PageHeader from '@/components/common/PageHeader.vue'
import {
  userApi,
  type DeviceInfo,
  type PurchaseInfo,
  type ActivationStatus,
  type RedeemResult,
} from '@/api/user'

const status = ref<ActivationStatus | null>(null)
const deviceInfo = ref<DeviceInfo | null>(null)
const purchase = ref<PurchaseInfo | null>(null)
const endpoint = ref('')

const voucherInput = ref('')
const submitting = ref(false)
const loading = ref(false)
const qrAvailable = ref(true)
/** 收款码放 frontend/public/pay-qr.png 即可生效,不放则显示文字引导。
 *  这里用变量绑定而不是字面量,避免 Vite 在构建期把它当模块去解析。 */
const qrSrc = '/pay-qr.png'

const manualBlob = ref('')
const manualSubmitting = ref(false)

const activePanels = ref<string[]>([])
const isActivated = computed(() => status.value?.activated ?? false)

// ---------------------------------------------------------------- 数据

async function refresh() {
  loading.value = true
  try {
    status.value = await userApi.checkActivation()
    deviceInfo.value = await userApi.getDeviceInfo()
    purchase.value = await userApi.getPurchaseInfo()
    endpoint.value = await userApi.redeemEndpoint()
  } catch (e: any) {
    ElMessage.error(`读取激活状态失败: ${errMsg(e)}`)
  } finally {
    loading.value = false
  }
}

// ---------------------------------------------------------------- 卡密

/** 归一化 + 分组显示。与内核 voucher::normalize 保持同一套规则 */
function formatCode(raw: string): string {
  let s = raw
    .toUpperCase()
    .replace(/[^0-9A-Z]/g, '')
    .replace(/O/g, '0')
    .replace(/[IL]/g, '1')
    .replace(/U/g, 'V')
  while (s.length > 20 && s.startsWith('DV')) s = s.slice(2)
  s = s.slice(0, 20)
  const groups = s.match(/.{1,5}/g) ?? []
  return groups.length ? `DV-${groups.join('-')}` : ''
}

function onCodeBlur() {
  if (voucherInput.value.trim()) voucherInput.value = formatCode(voucherInput.value)
}

async function doRedeem(isRebind = false): Promise<void> {
  const code = voucherInput.value.trim()
  if (!code) {
    ElMessage.warning('请先输入卡密')
    return
  }
  submitting.value = true
  try {
    const res: RedeemResult = isRebind
      ? await userApi.rebindVoucher(code)
      : await userApi.redeemVoucher(code)
    ElMessage.success(
      res.perpetual
        ? `${res.newly_bound ? '激活成功' : '已重新激活'}，永久授权，卡密尾号 ${res.code_suffix}`
        : `激活成功，有效期至 ${fmtDate(res.expiry)}`,
    )
    voucherInput.value = ''
    activePanels.value = []
    await refresh()
  } catch (e: any) {
    // 卡密已绑在别的机器上:引导走换机,而不是让用户以为卡密坏了
    if (!isRebind && errCode(e) === 'REDEEM_DEVICE_CONFLICT') {
      submitting.value = false
      if (await confirmRebind()) await doRedeem(true)
      return
    }
    ElMessage.error(errMsg(e))
  } finally {
    submitting.value = false
  }
}

async function confirmRebind(): Promise<boolean> {
  try {
    await ElMessageBox.confirm(
      '这张卡密已经绑定到另一台设备。把授权转移到本机会消耗 1 次换机机会，确定继续吗？',
      '需要换机',
      { confirmButtonText: '转移到本机', cancelButtonText: '取消', type: 'warning' },
    )
    return true
  } catch {
    return false
  }
}

// ---------------------------------------------------------------- 离线兜底

async function doActivateManual() {
  const blob = manualBlob.value.trim()
  if (!blob) {
    ElMessage.warning('请粘贴许可证')
    return
  }
  manualSubmitting.value = true
  try {
    await userApi.activate(blob)
    ElMessage.success('激活成功')
    manualBlob.value = ''
    await refresh()
  } catch (e: any) {
    ElMessage.error(`激活失败: ${errMsg(e)}`)
  } finally {
    manualSubmitting.value = false
  }
}

async function devGenerate() {
  try {
    manualBlob.value = await userApi.devGenerateLicense()
    ElMessage.info('已生成开发用许可证，点击「离线激活」即可')
  } catch (e: any) {
    ElMessage.error(`生成失败: ${errMsg(e)}`)
  }
}

// ---------------------------------------------------------------- 购买与复制

async function openPurchase() {
  const url = purchase.value?.purchase_url
  if (!url) return
  try {
    await open(url)
  } catch (e: any) {
    ElMessage.error(`无法打开浏览器，请手动访问: ${url}`)
  }
}

async function copyDeviceId() {
  const id = status.value?.device_id ?? deviceInfo.value?.device_id
  if (!id) return
  try {
    await navigator.clipboard.writeText(id)
    ElMessage.success('设备指纹已复制')
  } catch {
    ElMessage.warning('复制失败，请手动选中复制')
  }
}

// ---------------------------------------------------------------- 工具

function errCode(e: any): string {
  return typeof e === 'object' && e !== null ? String(e.code ?? '') : ''
}
function errMsg(e: any): string {
  if (typeof e === 'string') return e
  return e?.message ? String(e.message) : String(e)
}
function fmtDate(ts: number | null | undefined): string {
  if (!ts) return '—'
  return new Date(ts * 1000).toLocaleDateString('zh-CN')
}

onMounted(refresh)
</script>

<template>
  <div class="activation-view" v-loading="loading">
    <PageHeader
      title="激活"
      :subtitle="`${purchase?.price_label ?? '1 元永久激活'} · 无订阅 · 无会员分级 · 激活后完全离线`"
    />

    <!-- 状态条 -->
    <div class="status-card app-card">
      <el-tag :type="isActivated ? 'success' : 'warning'" size="large" effect="dark">
        {{ isActivated ? '已激活' : '未激活' }}
      </el-tag>
      <div class="status-detail" v-if="isActivated">
        <span>永久授权，激活于 {{ fmtDate(status?.activated_at) }}</span>
      </div>
      <div class="status-detail" v-else>
        <span>输入卡密即可解锁全部功能，一次买断，不续费</span>
      </div>
    </div>

    <!-- 主操作区 -->
    <div class="activate-grid">
      <div class="card-main app-card">
        <h3>{{ isActivated ? '更换卡密' : '输入卡密激活' }}</h3>
        <el-input
          v-model="voucherInput"
          size="large"
          maxlength="32"
          clearable
          placeholder="DV-XXXXX-XXXXX-XXXXX-XXXXX"
          class="code-input"
          @blur="onCodeBlur"
          @keyup.enter="doRedeem(false)"
        />
        <p class="hint">不区分大小写，分隔符可省略，粘贴后自动整理。付款后卡密会自动发送给你。</p>
        <div class="actions">
          <el-button
            type="primary"
            size="large"
            :loading="submitting"
            @click="doRedeem(false)"
          >
            {{ isActivated ? '用新卡密激活' : '激活' }}
          </el-button>
        </div>
      </div>

      <div class="card-side app-card">
        <h3>还没有卡密？</h3>
        <p class="price">{{ purchase?.price_label }}</p>
        <div class="qr-area">
          <img
            v-if="qrAvailable"
            :src="qrSrc"
            alt="收款码"
            class="qr-img"
            @error="qrAvailable = false"
          />
          <div v-else class="qr-fallback">
            <p>请在店铺页完成付款</p>
          </div>
        </div>
        <el-button class="buy-btn" @click="openPurchase">前往购买卡密</el-button>
        <p class="hint">付款后卡密自动发货，无需等待人工处理。</p>
      </div>
    </div>

    <!-- 换机说明（仅已激活时展开） -->
    <el-collapse v-model="activePanels" class="extra">
      <el-collapse-item title="换了电脑 / 重装了系统？" name="rebind">
        <p class="hint">
          授权绑定本机设备指纹。换机或重装系统后，在上面的输入框填入**原来那张卡密**，
          系统会提示是否把授权转移到本机（会消耗 1 次换机机会，默认共 3 次）。
          次数用完了请带上下面的设备指纹联系卖家。
        </p>
        <div class="device-line">
          <span class="label">当前设备指纹</span>
          <code>{{ status?.device_id }}</code>
          <el-button link type="primary" @click="copyDeviceId">复制</el-button>
        </div>
      </el-collapse-item>

      <el-collapse-item title="已有许可证文件（离线激活）" name="manual">
        <p class="hint">
          客服手工签发的许可证可直接粘贴到下方。正常购买流程不需要用到这里。
        </p>
        <el-input
          v-model="manualBlob"
          type="textarea"
          :rows="5"
          placeholder="粘贴许可证（base64 字符串）"
        />
        <div class="actions">
          <el-button type="primary" :loading="manualSubmitting" @click="doActivateManual">
            离线激活
          </el-button>
          <el-button v-if="!isActivated" @click="devGenerate">生成测试许可证（开发用）</el-button>
        </div>
      </el-collapse-item>
    </el-collapse>

    <!-- 设备信息 -->
    <div class="device-card app-card" v-if="deviceInfo">
      <h3>本机信息</h3>
      <el-descriptions :column="2" border size="small">
        <el-descriptions-item label="操作系统">{{ deviceInfo.os_name }}</el-descriptions-item>
        <el-descriptions-item label="系统版本">{{ deviceInfo.os_version }}</el-descriptions-item>
        <el-descriptions-item label="主机名">{{ deviceInfo.hostname }}</el-descriptions-item>
        <el-descriptions-item label="架构">{{ deviceInfo.arch }}</el-descriptions-item>
        <el-descriptions-item label="设备指纹" :span="2">
          <code class="mono">{{ deviceInfo.device_id }}</code>
        </el-descriptions-item>
      </el-descriptions>
      <p class="hint endpoint" v-if="endpoint">兑换服务: {{ endpoint }}</p>
    </div>
  </div>
</template>

<style scoped lang="scss">
.status-card {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 16px;
}
.status-detail {
  color: $text-regular;
  font-size: 13px;
}

.activate-grid {
  display: grid;
  grid-template-columns: 1fr 300px;
  gap: 16px;
  margin-bottom: 16px;
}

.card-main,
.card-side {
  h3 {
    margin: 0 0 12px;
    font-size: 15px;
    font-weight: 600;
  }
}

.code-input :deep(input) {
  font-family: monospace;
  letter-spacing: 1px;
  font-size: 16px;
}

.actions {
  margin-top: 16px;
  display: flex;
  gap: 8px;
}

.price {
  color: $police-blue;
  font-size: 20px;
  font-weight: 700;
  margin: 0 0 12px;
}

.qr-area {
  text-align: center;
  padding: 12px;
  background: #fff;
  border: 1px dashed $border-color;
  border-radius: $radius-base;
  margin-bottom: 12px;
  min-height: 120px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.qr-img {
  width: 160px;
  height: 160px;
  object-fit: contain;
}
.qr-fallback {
  color: $text-secondary;
  font-size: 12px;
}
.buy-btn {
  width: 100%;
}

.hint {
  font-size: 12px;
  color: $text-secondary;
  line-height: 1.6;
  margin: 8px 0 0;
}

.extra {
  margin-bottom: 16px;
  background: #fff;
  border-radius: $radius-base;
  padding: 0 16px;
}

.device-line {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: $text-secondary;
  margin-top: 8px;
  flex-wrap: wrap;
}

.device-card {
  h3 {
    margin: 0 0 12px;
    font-size: 15px;
    font-weight: 600;
  }
}

.mono {
  font-family: monospace;
  word-break: break-all;
}

.endpoint {
  word-break: break-all;
}
</style>
