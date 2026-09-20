import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'

const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/home' },
  {
    path: '/home',
    name: 'home',
    component: () => import('@/views/desktop/HomeView.vue'),
    meta: { title: '首页' },
  },
  {
    path: '/pdf-tools',
    name: 'pdf-tools',
    component: () => import('@/views/desktop/PdfToolsView.vue'),
    meta: { title: 'PDF 工具' },
  },
  {
    path: '/scan',
    name: 'scan',
    component: () => import('@/views/desktop/ScanView.vue'),
    meta: { title: '文档扫描' },
  },
  {
    path: '/idcard',
    name: 'idcard',
    component: () => import('@/views/desktop/IdCardView.vue'),
    meta: { title: '证件排版' },
  },
  {
    path: '/ocr',
    name: 'ocr',
    component: () => import('@/views/desktop/OcrView.vue'),
    meta: { title: 'OCR 识别' },
  },
  {
    path: '/word2pdf',
    name: 'word2pdf',
    component: () => import('@/views/desktop/WordToPdfView.vue'),
    meta: { title: 'Word 转 PDF' },
  },
  {
    path: '/images2pdf',
    name: 'images2pdf',
    component: () => import('@/views/desktop/ImagesToPdfView.vue'),
    meta: { title: '图片转 PDF' },
  },
  {
    path: '/documents',
    name: 'documents',
    component: () => import('@/views/desktop/DocumentListView.vue'),
    meta: { title: '文档管理' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('@/views/desktop/SettingsView.vue'),
    meta: { title: '设置' },
  },
  {
    path: '/activation',
    name: 'activation',
    component: () => import('@/views/desktop/ActivationView.vue'),
    meta: { title: '激活' },
  },
]

const router = createRouter({
  history: createWebHashHistory(),
  routes,
})

router.afterEach((to) => {
  const title = (to.meta.title as string) || 'PDF-DocVault'
  document.title = `${title} · PDF-DocVault`
})

export default router
