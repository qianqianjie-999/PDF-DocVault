import { beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useSettingsStore } from '@/stores/settings'

// 设置 store:localStorage 往返与损坏数据容错
describe('useSettingsStore', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
  })

  it('默认值正确', () => {
    const s = useSettingsStore()
    expect(s.defaultOutputDir).toBe('')
    expect(s.defaultDpi).toBe(150)
    expect(s.defaultImageFormat).toBe('png')
    expect(s.ocrLanguage).toBe('ch')
  })

  it('save 后 load 能恢复全部字段', () => {
    const s = useSettingsStore()
    s.defaultOutputDir = '/home/user/docs'
    s.defaultDpi = 300
    s.defaultImageFormat = 'jpg'
    s.ocrLanguage = 'en'
    s.save()

    // 新 store 实例模拟应用重启
    setActivePinia(createPinia())
    const s2 = useSettingsStore()
    s2.load()
    expect(s2.defaultOutputDir).toBe('/home/user/docs')
    expect(s2.defaultDpi).toBe(300)
    expect(s2.defaultImageFormat).toBe('jpg')
    expect(s2.ocrLanguage).toBe('en')
  })

  it('load 遇到损坏 JSON 不抛错且保留默认值', () => {
    localStorage.setItem('pdf-docvault-settings', '{broken json!!')
    const s = useSettingsStore()
    expect(() => s.load()).not.toThrow()
    expect(s.defaultDpi).toBe(150)
  })

  it('load 时缺省字段保留默认值', () => {
    localStorage.setItem('pdf-docvault-settings', JSON.stringify({ defaultDpi: 96 }))
    const s = useSettingsStore()
    s.load()
    expect(s.defaultDpi).toBe(96)
    expect(s.defaultOutputDir).toBe('')
    expect(s.ocrLanguage).toBe('ch')
  })

  it('autoOpenOutput 默认开启,且能显式持久化 false', () => {
    const s = useSettingsStore()
    expect(s.autoOpenOutput).toBe(true)

    // 显式关闭并保存
    s.autoOpenOutput = false
    s.save()

    // 新实例恢复 false(布尔 false 不能被当作缺省丢弃)
    setActivePinia(createPinia())
    const s2 = useSettingsStore()
    s2.load()
    expect(s2.autoOpenOutput).toBe(false)
  })

  it('autoOpenOutput 持久化 true 往返', () => {
    const s = useSettingsStore()
    s.autoOpenOutput = true
    s.save()
    setActivePinia(createPinia())
    const s2 = useSettingsStore()
    s2.load()
    expect(s2.autoOpenOutput).toBe(true)
  })
})
