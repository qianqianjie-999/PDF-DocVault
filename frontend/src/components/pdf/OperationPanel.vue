<script setup lang="ts">
import { ref, computed, reactive, watch } from 'vue'
import { open, save } from '@tauri-apps/plugin-dialog'
import { ElMessage } from 'element-plus'
import { pdfApi } from '@/api/pdf'
import { useFilesStore } from '@/stores/files'
import { useTaskStore } from '@/stores/task'
import { useSettingsStore } from '@/stores/settings'
import { basename, dirname, formatFileSize, joinPath, samePath } from '@/utils/file'
import {
  MAX_SPLIT_RANGES,
  pageInputInvalid,
  parsePageNums,
  parseRanges,
} from '@/utils/pages'
import { openOutput } from '@/utils/openOutput'
import ProgressOverlay from './ProgressOverlay.vue'
import type { EncryptionInfo, SplitMode } from '@/types/pdf'

const filesStore = useFilesStore()
const taskStore = useTaskStore()
const settings = useSettingsStore()
const running = ref(false)
const progress = ref(0)
const current = ref(0)
const total = ref(0)
const showProgress = ref(false)

type OpType =
  | 'merge'
  | 'split'
  | 'rotate'
  | 'delete'
  | 'extract'
  | 'encrypt'
  | 'decrypt'
  | 'watermark'
  | 'compress'
  | 'toImages'
  | 'reorder'

const op = ref<OpType>('merge')

const operations: { key: OpType; label: string; icon: string }[] = [
  { key: 'merge', label: '合并', icon: '🔗' },
  { key: 'split', label: '拆分', icon: '✂️' },
  { key: 'rotate', label: '旋转', icon: '🔄' },
  { key: 'delete', label: '删除页', icon: '🗑️' },
  { key: 'extract', label: '提取页', icon: '📤' },
  { key: 'reorder', label: '重排', icon: '🔀' },
  { key: 'encrypt', label: '加密', icon: '🔒' },
  { key: 'decrypt', label: '解密', icon: '🔓' },
  { key: 'watermark', label: '水印', icon: '💧' },
  { key: 'compress', label: '压缩', icon: '📦' },
  { key: 'toImages', label: '转图片', icon: '🖼️' },
]

// 参数
const splitMode = ref<'ranges' | 'everyN'>('everyN')
const splitN = ref(50)
const splitRanges = ref('1-10')
const rotateDegrees = ref(90)
const pageInput = ref('') // 旋转/删除/提取通用页码输入
const reorderInput = ref('') // 重排新顺序输入
const encryptPassword = ref('')
const encryptBits = ref(256)
// 权限默认:允许打印,禁止复制/修改/批注(文档保护场景的常见预期)
const encryptPermissions = reactive({
  allow_print: true,
  allow_copy: false,
  allow_modify: false,
  allow_annotate: false,
})
const decryptPassword = ref('')
const encryptProbe = ref<EncryptionInfo | null>(null)

// 切到加密/解密/旋转/删除/提取/水印/压缩面板或换文件时探测一次:
// - 加密页据此提示「已加密,请先解密」,否则用户点加密只会收到「密码错误」;
// - 解密页据此区分「需要打开密码」与「仅限制权限」,后者允许空密码直接解密;
// - 其余单文件页据此提前拦截加密 PDF,避免用户选完输出路径才报错。
const ENCRYPT_PROBE_OPS: OpType[] = ['encrypt', 'decrypt', 'rotate', 'delete', 'extract', 'watermark', 'compress', 'reorder']
watch(
  () => [op.value, filesStore.selectedFiles[0]?.path] as const,
  async () => {
    encryptProbe.value = null
    if (!ENCRYPT_PROBE_OPS.includes(op.value) || filesStore.selectedFiles.length !== 1) return
    try {
      encryptProbe.value = await pdfApi.probeEncryption(filesStore.selectedFiles[0].path)
    } catch {
      encryptProbe.value = null // 探测失败不打扰用户,真正处理时再报错
    }
  },
  { immediate: true },
)
const watermarkText = ref('PDF-DocVault')
// 图片水印后端接口早已就绪,此前仅缺前端入口;文字/图片二选一
const watermarkType = ref<'text' | 'image'>('text')
const watermarkImagePath = ref('')
const watermarkFontSize = ref(48)
const watermarkOpacity = ref(0.3)
const watermarkRotation = ref(45)

/** 选择图片作为水印;清空时水印类型自动回到文字 */
async function pickWatermarkImage() {
  const selected = await open({
    multiple: false,
    filters: [{ name: '图片', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp'] }],
  })
  if (typeof selected === 'string') watermarkImagePath.value = selected
}
const compressLevel = ref(6)
// 转图片默认值跟随设置页,避免那几项开关形同虚设
const imagesDpi = ref(settings.defaultDpi)
const imagesFormat = ref<'png' | 'jpg' | 'webp'>(settings.defaultImageFormat)
const imagesPages = ref('') // 转图片页码范围输入,留空 = 整本导出
// 设置页改默认值后同步过来:面板只挂载一次,不监听的话改完设置回来仍是旧值
watch(
  () => [settings.defaultDpi, settings.defaultImageFormat] as const,
  ([dpi, fmt]) => {
    imagesDpi.value = dpi
    imagesFormat.value = fmt
  },
)

// 需要单文件的操作
const needsSingleFile = computed(() =>
  ['split', 'rotate', 'delete', 'extract', 'encrypt', 'decrypt', 'watermark', 'compress', 'toImages', 'reorder'].includes(
    op.value,
  ),
)

// 需要多文件的操作
const needsMultiFile = computed(() => op.value === 'merge')

const canRun = computed(() => {
  if (running.value) return false
  if (needsSingleFile.value) return filesStore.selectedFiles.length === 1
  if (needsMultiFile.value) return filesStore.selectedFiles.length >= 2
  return filesStore.selectedFiles.length > 0
})

async function pickOutput(
  defaultName: string,
  inputs?: string | string[],
): Promise<string | null> {
  // 设置页配置的默认输出目录优先生效,留空则由系统记住上次位置
  const dir = settings.defaultOutputDir.trim()
  const out = await save({
    defaultPath: dir ? joinPath(dir, defaultName) : defaultName,
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  })
  if (!out) return null
  // 保存对话框允许选中已有文件,若恰好是源文件,后续会静默覆盖原件(加密/解密等
  // 路径甚至可能直接失败),这里统一拦截并提示用户换名
  const sources = inputs === undefined ? [] : Array.isArray(inputs) ? inputs : [inputs]
  if (sources.some((p) => samePath(out, p))) {
    ElMessage.warning('输出文件不能与源文件相同,请更换文件名或目录')
    return null
  }
  return out
}

async function run() {
  if (!canRun.value) {
    ElMessage.warning(needsSingleFile.value ? '请选择一个文件' : '请选择至少两个文件')
    return
  }
  const files = filesStore.selectedFiles
  running.value = true
  showProgress.value = true
  progress.value = 0
  // 清空上一轮计数,否则转图片之后再执行其他操作会显示残留的「x / y」
  current.value = 0
  total.value = 0
  try {
    switch (op.value) {
      case 'merge': {
        const out = await pickOutput(
          'merged.pdf',
          files.map((f) => f.path),
        )
        if (!out) break
        const res = await pdfApi.merge(files.map((f) => f.path), out)
        ElMessage.success(`合并完成: ${res.pages} 页,耗时 ${res.elapsed_ms}ms`)
        openOutput({ output: out })
        break
      }
      case 'split': {
        const first = files[0].path
        // 设置页配置了默认输出目录时优先使用,否则输出到源文件旁
        const dir = settings.defaultOutputDir.trim() || dirname(first)
        let mode: SplitMode
        if (splitMode.value === 'everyN') {
          if (!splitN.value || splitN.value < 1) {
            ElMessage.warning('请填写大于 0 的拆分页数')
            break
          }
          mode = { kind: 'every_n_pages', n: splitN.value }
        } else {
          // 每段都会生成一个文件,段数过多时先拦下并给出明确提示
          const parts = splitRanges.value.split(',').filter((p) => p.trim()).length
          if (parts > MAX_SPLIT_RANGES) {
            ElMessage.warning(`一次最多拆分 ${MAX_SPLIT_RANGES} 段,请减少范围数量`)
            break
          }
          const ranges = parseRanges(splitRanges.value)
          // 输入全为无效片段时提前提示,避免用户选完文件才收到后端报错
          if (!ranges.length) {
            ElMessage.warning('请输入有效的页码范围,如 1-10,11-20')
            break
          }
          mode = { kind: 'by_ranges', ranges }
        }
        const res = await pdfApi.split(first, mode, dir)
        ElMessage.success(`拆分完成: ${res.outputs.length} 个文件`)
        openOutput({ dir, firstOutput: res.outputs[0] })
        break
      }
      case 'rotate': {
        if (pageInputInvalid(pageInput.value)) {
          ElMessage.warning('页码格式有误,请使用如 1,3,5-8 的写法')
          break
        }
        // 需要打开密码的加密 PDF 无法旋转,提前给出与「解密」一致的提示
        if (encryptProbe.value?.locked) {
          ElMessage.warning('该文件已加密,请先在「解密」中去除原有加密')
          break
        }
        const first = files[0].path
        const out = await pickOutput(`rotated-${basename(first)}`, first)
        if (!out) break
        await pdfApi.rotate(first, parsePageNums(pageInput.value), rotateDegrees.value, out)
        ElMessage.success('旋转完成')
        openOutput({ output: out })
        break
      }
      case 'delete': {
        if (pageInputInvalid(pageInput.value)) {
          ElMessage.warning('页码格式有误,请使用如 1,3,5-8 的写法')
          break
        }
        if (encryptProbe.value?.locked) {
          ElMessage.warning('该文件已加密,请先在「解密」中去除原有加密')
          break
        }
        const first = files[0].path
        const out = await pickOutput(`trimmed-${basename(first)}`, first)
        if (!out) break
        await pdfApi.deletePages(first, parsePageNums(pageInput.value), out)
        ElMessage.success('删除页面完成')
        openOutput({ output: out })
        break
      }
      case 'extract': {
        if (pageInputInvalid(pageInput.value)) {
          ElMessage.warning('页码格式有误,请使用如 1,3,5-8 的写法')
          break
        }
        if (encryptProbe.value?.locked) {
          ElMessage.warning('该文件已加密,请先在「解密」中去除原有加密')
          break
        }
        const first = files[0].path
        const out = await pickOutput(`extracted-${basename(first)}`, first)
        if (!out) break
        await pdfApi.extract(first, parsePageNums(pageInput.value), out)
        ElMessage.success('提取完成')
        openOutput({ output: out })
        break
      }
      case 'reorder': {
        const first = files[0].path
        const order = parsePageNums(reorderInput.value)
        if (!order.length) {
          ElMessage.warning('请输入新的页面顺序,如 3,1,2,4(需覆盖全部页面)')
          break
        }
        if (encryptProbe.value?.locked) {
          ElMessage.warning('该文件已加密,请先在「解密」中去除原有加密')
          break
        }
        const out = await pickOutput(`reordered-${basename(first)}`, first)
        if (!out) break
        await pdfApi.reorder(first, order, out)
        ElMessage.success('重排完成')
        openOutput({ output: out })
        break
      }
      case 'encrypt': {
        // 探测确认源文件已加密时直接拦下:加密链路不会先解密输入,
        // 继续执行只会得到与「设置新密码」无关的密码错误提示
        if (encryptProbe.value?.encrypted) {
          ElMessage.warning('该文件已加密,请先在「解密」中去除原有加密')
          break
        }
        const pw = encryptPassword.value.trim()
        if (!pw) {
          ElMessage.warning('请先设置打开密码')
          break
        }
        // AES-128(R4) 的密码受 PDFDocEncoding 限制,含中文会直接失败
        if (encryptBits.value === 128 && /[^\x00-\x7F]/.test(pw)) {
          ElMessage.warning('AES-128 不支持中文密码,请改用 AES-256')
          break
        }
        const first = files[0].path
        const out = await pickOutput(`encrypted-${basename(first)}`, first)
        if (!out) break
        const rep = await pdfApi.encrypt(first, out, {
          user_password: pw,
          owner_password: null,
          permissions: { ...encryptPermissions },
          bits: encryptBits.value,
        })
        ElMessage.success(
          `加密完成:${rep.pages} 页 · AES-${rep.bits}(R${rep.revision}) · ${formatFileSize(rep.size)}`,
        )
        openOutput({ output: out })
        break
      }
      case 'decrypt': {
        // 只有探测确认需要打开密码时才强制输入。仅限制权限的加密 PDF(用户密码为空)
        // 可以留空直接解密;探测失败时也放行,由 pycore 给出真正的失败原因。
        if (encryptProbe.value?.locked) {
          ElMessage.warning('请输入该 PDF 的打开密码')
          break
        }
        const first = files[0].path
        const out = await pickOutput(`decrypted-${basename(first)}`, first)
        if (!out) break
        const rep = await pdfApi.decrypt(first, out, decryptPassword.value)
        ElMessage.success(
          rep.was_encrypted
            ? `解密完成:${rep.pages} 页 · ${formatFileSize(rep.size)}`
            : `该文件本来就没有加密,已原样另存:${rep.pages} 页`,
        )
        openOutput({ output: out })
        break
      }
      case 'watermark': {
        if (watermarkType.value === 'image' && !watermarkImagePath.value) {
          ElMessage.warning('请先选择作为水印的图片')
          break
        }
        if (watermarkType.value === 'text' && !watermarkText.value.trim()) {
          ElMessage.warning('请输入水印文本')
          break
        }
        if (encryptProbe.value?.locked) {
          ElMessage.warning('该文件已加密,请先在「解密」中去除原有加密')
          break
        }
        const first = files[0].path
        const out = await pickOutput(`watermarked-${basename(first)}`, first)
        if (!out) break
        await pdfApi.watermark(first, out, {
          text: watermarkType.value === 'text' ? watermarkText.value : undefined,
          image_path: watermarkType.value === 'image' ? watermarkImagePath.value : undefined,
          options: {
            font_size: watermarkFontSize.value,
            rotation: watermarkRotation.value,
            opacity: watermarkOpacity.value,
            color: [0.5, 0.5, 0.5],
            x: 150,
            y: 300,
          },
        })
        ElMessage.success('水印添加完成')
        openOutput({ output: out })
        break
      }
      case 'compress': {
        if (encryptProbe.value?.locked) {
          ElMessage.warning('该文件已加密,请先在「解密」中去除原有加密')
          break
        }
        const first = files[0].path
        const out = await pickOutput(`compressed-${basename(first)}`, first)
        if (!out) break
        const res = await pdfApi.compress(first, out, compressLevel.value)
        if (res.fell_back) {
          ElMessage.warning(`已按结构模式压缩: 节省 ${res.ratio.toFixed(1)}%(图片处理已回退)`)
        } else if (res.images_reencoded > 0) {
          ElMessage.success(`压缩完成: 重编码 ${res.images_reencoded} 张图,节省 ${res.ratio.toFixed(1)}%`)
        } else {
          ElMessage.success(`结构压缩完成: 节省 ${res.ratio.toFixed(1)}%`)
        }
        openOutput({ output: out })
        break
      }
      case 'toImages': {
        if (pageInputInvalid(imagesPages.value)) {
          ElMessage.warning('页码格式有误,请使用如 1,3,5-8 的写法')
          break
        }
        const first = files[0].path
        const dir = settings.defaultOutputDir.trim() || dirname(first)
        const taskName = `to-images-${Date.now()}`
        // 页码输入解析:留空 = 整本导出,否则展开为离散页列表(去重排序由后端完成)
        const pages = parsePageNums(imagesPages.value)
        // 监听进度
        taskStore.init()
        const unlisten = await (await import('@tauri-apps/api/event')).listen<{
          task_id: string
          percent: number
          current: number
          total: number
        }>('task-progress', (e) => {
          if (e.payload.task_id === taskName) {
            progress.value = e.payload.percent
            current.value = e.payload.current
            total.value = e.payload.total
          }
        })
        const res = await pdfApi.toImages(
          first,
          dir,
          imagesDpi.value,
          imagesFormat.value,
          taskName,
          pages.length > 0 ? pages : undefined,
        ).finally(unlisten)
        progress.value = 100
        ElMessage.success(`转图片完成: ${res.count} 张`)
        openOutput({ dir, firstOutput: res.outputs[0] })
        break
      }
    }
  } catch (e: any) {
    ElMessage.error(`处理失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    // 用户可能在隐藏延迟内又开始下一轮,此时不能再把新一轮的进度层收掉
    setTimeout(() => {
      if (!running.value) showProgress.value = false
    }, 800)
  }
}
</script>

<template>
  <div class="operation-panel app-card">
    <div class="op-tabs">
      <div
        v-for="o in operations"
        :key="o.key"
        class="op-tab"
        :class="{ active: op === o.key }"
        @click="op = o.key"
      >
        <span class="op-icon">{{ o.icon }}</span>
        <span class="op-label">{{ o.label }}</span>
      </div>
    </div>

    <div class="op-form">
      <!-- 合并:无需额外参数 -->
      <div v-if="op === 'merge'" class="form-tip">
        将选中的多个 PDF 按顺序合并为一个文件
      </div>

      <!-- 拆分 -->
      <el-form v-else-if="op === 'split'" label-width="100px" label-position="left">
        <el-form-item label="拆分方式">
          <el-radio-group v-model="splitMode">
            <el-radio value="everyN">每 N 页</el-radio>
            <el-radio value="ranges">按页码范围</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item v-if="splitMode === 'everyN'" label="每页数">
          <el-input-number v-model="splitN" :min="1" :max="999" />
        </el-form-item>
        <el-form-item v-else label="页码范围">
          <el-input v-model="splitRanges" placeholder="如 1-10,11-20,21-30" />
        </el-form-item>
      </el-form>

      <!-- 旋转 -->
      <el-form v-else-if="op === 'rotate'" label-width="100px" label-position="left">
        <el-form-item label="页码">
          <el-input v-model="pageInput" placeholder="如 1,3,5(留空表示全部)" />
        </el-form-item>
        <el-form-item label="旋转角度">
          <el-radio-group v-model="rotateDegrees">
            <el-radio-button :value="90">90°</el-radio-button>
            <el-radio-button :value="180">180°</el-radio-button>
            <el-radio-button :value="270">270°</el-radio-button>
          </el-radio-group>
        </el-form-item>
      </el-form>

      <!-- 删除/提取 -->
      <el-form v-else-if="op === 'delete' || op === 'extract'" label-width="100px" label-position="left">
        <el-form-item label="页码">
          <el-input v-model="pageInput" placeholder="如 1,3,5-8" />
        </el-form-item>
      </el-form>

      <!-- 重排 -->
      <el-form v-else-if="op === 'reorder'" label-width="100px" label-position="left">
        <el-form-item label="新顺序">
          <el-input v-model="reorderInput" placeholder="如 3,1,2,4(需覆盖全部页面,1-based)" />
        </el-form-item>
        <el-form-item>
          <span class="hint">用逗号分隔原页码,按此顺序输出全部页面</span>
        </el-form-item>
      </el-form>

      <!-- 加密 -->
      <el-form v-else-if="op === 'encrypt'" label-width="100px" label-position="left">
        <el-alert
          v-if="encryptProbe?.encrypted"
          type="warning"
          :closable="false"
          show-icon
          title="该文件已加密"
          description="请先在「解密」中去除原有加密,再设置新密码"
          class="form-alert"
        />
        <el-form-item label="密码">
          <el-input v-model="encryptPassword" type="password" show-password placeholder="打开文档时需要输入" />
        </el-form-item>
        <el-form-item label="加密位数">
          <el-radio-group v-model="encryptBits">
            <el-radio :value="128">128-bit AES</el-radio>
            <el-radio :value="256">256-bit AES</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="权限">
          <div class="perm-grid">
            <el-checkbox v-model="encryptPermissions.allow_print">允许打印</el-checkbox>
            <el-checkbox v-model="encryptPermissions.allow_copy">允许复制文本</el-checkbox>
            <el-checkbox v-model="encryptPermissions.allow_modify">允许修改内容</el-checkbox>
            <el-checkbox v-model="encryptPermissions.allow_annotate">允许批注</el-checkbox>
          </div>
        </el-form-item>
        <el-form-item v-if="encryptBits === 128">
          <div class="hint">AES-128 仅支持字母数字密码;需要中文密码请选 AES-256</div>
        </el-form-item>
      </el-form>

      <!-- 解密 -->
      <el-form v-else-if="op === 'decrypt'" label-width="100px" label-position="left">
        <el-alert
          v-if="encryptProbe && !encryptProbe.encrypted"
          type="info"
          :closable="false"
          show-icon
          title="该文件未加密"
          description="解密只会另存一份副本"
          class="form-alert"
        />
        <el-alert
          v-else-if="encryptProbe?.encrypted && !encryptProbe.locked"
          type="info"
          :closable="false"
          show-icon
          title="该文件仅限制权限"
          description="无需打开密码,密码留空即可直接解密"
          class="form-alert"
        />
        <el-form-item label="密码">
          <el-input v-model="decryptPassword" type="password" show-password />
        </el-form-item>
      </el-form>

      <!-- 水印 -->
      <el-form v-else-if="op === 'watermark'" label-width="100px" label-position="left">
        <el-form-item label="水印类型">
          <el-radio-group v-model="watermarkType">
            <el-radio-button value="text">文字</el-radio-button>
            <el-radio-button value="image">图片</el-radio-button>
          </el-radio-group>
        </el-form-item>
        <template v-if="watermarkType === 'text'">
          <el-form-item label="水印文字">
            <el-input v-model="watermarkText" />
          </el-form-item>
          <el-form-item label="字号">
            <el-input-number v-model="watermarkFontSize" :min="8" :max="144" />
          </el-form-item>
          <el-form-item label="旋转">
            <el-slider v-model="watermarkRotation" :min="0" :max="360" />
          </el-form-item>
        </template>
        <template v-else>
          <el-form-item label="水印图片">
            <div class="wm-image-row">
              <el-button @click="pickWatermarkImage">选择图片</el-button>
              <span class="wm-image-name">{{ watermarkImagePath ? basename(watermarkImagePath) : '未选择' }}</span>
            </div>
          </el-form-item>
          <el-form-item label="图片宽度">
            <el-input-number v-model="watermarkFontSize" :min="8" :max="144" />
            <span class="hint">单位为磅,实际宽度 ≈ 数值 × 2</span>
          </el-form-item>
        </template>
        <el-form-item label="不透明度">
          <el-slider v-model="watermarkOpacity" :min="0" :max="1" :step="0.05" />
        </el-form-item>
      </el-form>

      <!-- 压缩 -->
      <el-form v-else-if="op === 'compress'" label-width="100px" label-position="left">
        <el-form-item label="压缩级别">
          <el-slider v-model="compressLevel" :min="1" :max="9" :marks="{ 1: '无损', 4: '均衡', 9: '最小' }" />
        </el-form-item>
        <el-form-item>
          <span class="compress-hint">
            1–3 仅清理冗余结构(无损);4–9 重编码内嵌图片,体积更小但画质有损。
          </span>
        </el-form-item>
      </el-form>

      <!-- 转图片 -->
      <el-form v-else-if="op === 'toImages'" label-width="100px" label-position="left">
        <el-form-item label="页码范围">
          <el-input v-model="imagesPages" placeholder="如 1,3,5-8(留空表示全部)" />
        </el-form-item>
        <el-form-item label="DPI">
          <el-input-number v-model="imagesDpi" :min="72" :max="600" :step="72" />
        </el-form-item>
        <el-form-item label="格式">
          <el-radio-group v-model="imagesFormat">
            <el-radio-button value="png">PNG</el-radio-button>
            <el-radio-button value="jpg">JPG</el-radio-button>
            <el-radio-button value="webp">WebP</el-radio-button>
          </el-radio-group>
        </el-form-item>
      </el-form>

      <div class="run-bar">
        <el-button
          type="primary"
          size="large"
          :loading="running"
          :disabled="!canRun"
          @click="run"
        >
          {{ running ? '处理中...' : '开始处理' }}
        </el-button>
        <span v-if="needsSingleFile" class="hint">需要选中 1 个文件</span>
        <span v-else-if="needsMultiFile" class="hint">需要选中 ≥2 个文件</span>
      </div>
    </div>

    <ProgressOverlay
      :visible="showProgress"
      :percent="progress"
      :current="current"
      :total="total"
    />
  </div>
</template>

<style scoped lang="scss">

.operation-panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.op-tabs {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(90px, 1fr));
  gap: 8px;
}
.op-tab {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 10px 4px;
  border: 1px solid $border-color;
  border-radius: $radius-base;
  cursor: pointer;
  transition: all 0.2s;
  &:hover {
    border-color: $police-blue;
    color: $police-blue;
  }
  &.active {
    border-color: $police-blue;
    background: $police-blue;
    color: #fff;
  }
}
.op-icon {
  font-size: 20px;
}
.op-label {
  font-size: 12px;
  margin-top: 4px;
}
.op-form {
  min-height: 120px;
}
.form-tip {
  color: $text-secondary;
  padding: 20px 0;
}
.run-bar {
  margin-top: 20px;
  display: flex;
  align-items: center;
  gap: 12px;
}
.hint {
  font-size: 12px;
  color: $text-secondary;
}
.perm-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2px 12px;
}
.form-alert {
  margin-bottom: 12px;
}
.compress-hint {
  font-size: 12px;
  color: $text-secondary;
  line-height: 1.5;
}
.wm-image-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
.wm-image-name {
  font-size: 12px;
  color: $text-secondary;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 240px;
}
</style>
