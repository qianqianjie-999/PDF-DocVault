import { call } from './index'

export interface ActivationStatus {
  activated: boolean
  plan: string | null
  device_id: string
}

export interface DeviceInfo {
  device_id: string
  os_name: string
  os_version: string
  hostname: string
  cpu_brand: string
  arch: string
}

export interface DonationQrCode {
  qr_base64: string
  activate_url: string
  device_id: string
  amount_label: string
}

export const userApi = {
  checkActivation: () => call<ActivationStatus>('check_activation'),
  activate: (licenseBlob: string) => call<boolean>('activate', { license_blob: licenseBlob }),
  getDonationQrcode: () => call<DonationQrCode>('get_donation_qrcode'),
  getDeviceInfo: () => call<DeviceInfo>('get_device_info'),
  devGenerateLicense: () => call<string>('dev_generate_license'),
}
