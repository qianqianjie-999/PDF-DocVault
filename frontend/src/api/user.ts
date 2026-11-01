import { call } from './index'

export interface ActivationStatus {
  activated: boolean
  plan: string | null
  device_id: string
  /** 激活时间戳(秒),未激活为 null */
  activated_at: number | null
  /** 过期时间戳(秒),0 表示永久 */
  expiry: number | null
  perpetual: boolean
}

export interface DeviceInfo {
  device_id: string
  os_name: string
  os_version: string
  hostname: string
  cpu_brand: string
  arch: string
}

export interface PurchaseInfo {
  price_yuan: number
  price_label: string
  purchase_url: string
  device_id: string
}

export interface RedeemResult {
  plan: string
  expiry: number
  perpetual: boolean
  /** 剩余换机次数 */
  rebind_left: number
  /** true = 本次首次绑定;false = 同设备重新激活 */
  newly_bound: boolean
  /** 卡密尾号,便于用户确认用的是哪张卡 */
  code_suffix: string
}

export const userApi = {
  checkActivation: () => call<ActivationStatus>('check_activation'),
  /** 离线兜底:直接粘贴客服发来的许可证 blob */
  activate: (licenseBlob: string) => call<boolean>('activate', { license_blob: licenseBlob }),
  getPurchaseInfo: () => call<PurchaseInfo>('get_purchase_info'),
  getDeviceInfo: () => call<DeviceInfo>('get_device_info'),
  devGenerateLicense: () => call<string>('dev_generate_license'),
  /** 用卡密激活本设备 */
  redeemVoucher: (code: string) => call<RedeemResult>('redeem_voucher', { code }),
  /** 换机:把已绑定的卡密改绑到本设备 */
  rebindVoucher: (code: string) => call<RedeemResult>('rebind_voucher', { code }),
  /** 当前兑换服务地址(排障用) */
  redeemEndpoint: () => call<string>('redeem_endpoint'),
}
