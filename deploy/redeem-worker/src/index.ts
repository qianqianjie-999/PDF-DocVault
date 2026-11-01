/**
 * PDF-DocVault 卡密兑换与许可证签发服务
 *
 * 职责边界很清楚:这里只做两件事 —— 验卡、签名。
 * - 卡密(KV 中的 voucher:<code>)记录"这张码卖给了谁、绑在哪台机器"
 * - 私钥只存在于 Worker secret 中,客户端只有公钥,逆向客户端伪造不出许可证
 *
 * 接口:
 *   POST /redeem  { code, device_id }  首次兑换,把卡密绑定到设备并签发许可证
 *   POST /rebind  { code, device_id }  换机/重装,把已绑定卡密改绑到新设备(限次)
 *   GET  /health                       健康检查
 */

import * as ed from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha512';

// @noble/ed25519 v2 需要显式提供 sha512 实现
ed.etc.sha512Async = async (...messages: Uint8Array[]) =>
  sha512(ed.etc.concatBytes(...messages));

interface Env {
  VOUCHERS: KVNamespace;
  /** 32 字节 ed25519 种子,hex 编码。由 `wrangler secret put` 写入 */
  LICENSE_SIGNING_KEY: string;
  MAX_REBIND?: string;
  PLAN?: string;
  LICENSE_TTL?: string;
}

interface VoucherRecord {
  status: 'unused' | 'bound';
  device_id?: string;
  bound_at?: number;
  rebind_count?: number;
  batch?: string;
}

const CODE_CHARS = 20;
const PREFIX = 'DV';
const ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

// ------------------------------------------------------------------ 入口

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: corsHeaders() });
    }
    if (url.pathname === '/health') {
      return json({ ok: true, service: 'docvault-redeem' });
    }
    if (request.method !== 'POST') {
      return json({ error: 'method_not_allowed', message: '仅支持 POST' }, 405);
    }

    let body: { code?: string; device_id?: string };
    try {
      body = (await request.json()) as { code?: string; device_id?: string };
    } catch {
      return json({ error: 'bad_request', message: '请求体不是合法 JSON' }, 400);
    }

    const code = normalizeCode(body.code ?? '');
    const deviceId = (body.device_id ?? '').trim().toLowerCase();

    if (!isPlausibleCode(code)) {
      return json(
        { error: 'invalid_code_format', message: '卡密格式不正确,请核对后重试' },
        400,
      );
    }
    if (!/^[0-9a-f]{64}$/.test(deviceId)) {
      return json({ error: 'invalid_device', message: '设备指纹格式不正确' }, 400);
    }

    try {
      switch (url.pathname) {
        case '/redeem':
          return await handleRedeem(env, code, deviceId);
        case '/rebind':
          return await handleRebind(env, code, deviceId);
        default:
          return json({ error: 'not_found', message: '未知接口' }, 404);
      }
    } catch (e) {
      // 不把内部错误细节回给客户端,避免泄露 KV/签名实现
      console.error('unhandled', url.pathname, e);
      return json({ error: 'internal', message: '服务暂时不可用,请稍后重试' }, 500);
    }
  },
};

// ------------------------------------------------------------------ 兑换

async function handleRedeem(env: Env, code: string, deviceId: string): Promise<Response> {
  const key = storageKey(code);
  const raw = await env.VOUCHERS.get(key);
  if (raw === null) {
    return json(
      { error: 'voucher_not_found', message: '卡密不存在,请检查是否输错或联系卖家' },
      404,
    );
  }

  const record = parseRecord(raw);

  // 已绑定到同一台设备:幂等重签(用户重装、重激活都走这条)
  if (record.status === 'bound' && record.device_id === deviceId) {
    return await issue(env, code, deviceId, record);
  }

  // 已绑定到别的设备:不直接改绑,要求走 /rebind 以便计数并留下痕迹
  if (record.status === 'bound') {
    return json(
      {
        error: 'device_conflict',
        message: '该卡密已绑定其他设备。换电脑或重装系统后,请使用【换机】功能',
      },
      409,
    );
  }

  const updated: VoucherRecord = {
    status: 'bound',
    device_id: deviceId,
    bound_at: nowSec(),
    rebind_count: 0,
    batch: record.batch,
  };
  await env.VOUCHERS.put(key, JSON.stringify(updated));
  return await issue(env, code, deviceId, updated, true);
}

// ------------------------------------------------------------------ 换机

async function handleRebind(env: Env, code: string, deviceId: string): Promise<Response> {
  const key = storageKey(code);
  const raw = await env.VOUCHERS.get(key);
  if (raw === null) {
    return json(
      { error: 'voucher_not_found', message: '卡密不存在,请检查是否输错或联系卖家' },
      404,
    );
  }

  const record = parseRecord(raw);
  if (record.status !== 'bound' || !record.device_id) {
    // 从未绑定过的卡密直接走兑换逻辑,不必占用换机次数
    return await handleRedeem(env, code, deviceId);
  }

  if (record.device_id === deviceId) {
    return await issue(env, code, deviceId, record);
  }

  const maxRebind = Number(env.MAX_REBIND ?? '3');
  const used = record.rebind_count ?? 0;
  if (used >= maxRebind) {
    return json(
      {
        error: 'rebind_exhausted',
        message: `该卡密的换机次数已用完(${maxRebind} 次),请联系卖家处理`,
      },
      429,
    );
  }

  const updated: VoucherRecord = {
    ...record,
    device_id: deviceId,
    rebind_count: used + 1,
  };
  await env.VOUCHERS.put(key, JSON.stringify(updated));

  return await issue(env, code, deviceId, updated, true);
}

// ------------------------------------------------------------------ 签发

async function issue(
  env: Env,
  code: string,
  deviceId: string,
  record: VoucherRecord,
  freshlyBound = false,
): Promise<Response> {
  const seed = parseSeed(env.LICENSE_SIGNING_KEY);
  const plan = env.PLAN ?? 'full';
  const ttl = Number(env.LICENSE_TTL ?? '0');
  const activatedAt = record.bound_at ?? nowSec();
  const expiry = ttl > 0 ? activatedAt + ttl : 0;

  // 消息格式必须与 Rust 端 License::sign_message 完全一致:
  //   {device_id}|{plan}|{activated_at}|{expiry}
  const message = `${deviceId}|${plan}|${activatedAt}|${expiry}`;
  const signature = await ed.signAsync(new TextEncoder().encode(message), seed);

  const license = {
    device_id: deviceId,
    plan,
    activated_at: activatedAt,
    expiry,
    signature: b64encode(signature),
  };
  const blob = b64encode(new TextEncoder().encode(JSON.stringify(license)));

  return json({
    license: blob,
    plan,
    device_id: deviceId,
    expiry,
    perpetual: expiry === 0,
    rebind_left: Math.max(0, Number(env.MAX_REBIND ?? '3') - (record.rebind_count ?? 0)),
    newly_bound: freshlyBound,
    code_suffix: code.slice(-4),
  });
}

// ------------------------------------------------------------------ 工具

function normalizeCode(input: string): string {
  let out = '';
  for (const ch of input) {
    if (!/[0-9a-zA-Z]/.test(ch)) continue;
    const c = ch.toUpperCase();
    // 与 Rust 端 voucher::normalize 保持一致的易混字符归一
    out += c === 'O' ? '0' : c === 'I' || c === 'L' ? '1' : c === 'U' ? 'V' : c;
  }
  while (out.length > CODE_CHARS && out.startsWith(PREFIX)) {
    out = out.slice(PREFIX.length);
  }
  return out;
}

function isPlausibleCode(code: string): boolean {
  if (code.length !== CODE_CHARS) return false;
  return [...code].every((c) => ALPHABET.includes(c));
}

function storageKey(code: string): string {
  return `voucher:${code}`;
}

function parseRecord(raw: string): VoucherRecord {
  try {
    const parsed = JSON.parse(raw) as VoucherRecord;
    if (parsed.status === 'bound') return parsed;
    return { status: 'unused' };
  } catch {
    return { status: 'unused' };
  }
}

function parseSeed(hex: string): Uint8Array {
  const clean = (hex ?? '').trim();
  if (!/^[0-9a-fA-F]{64}$/.test(clean)) {
    throw new Error('LICENSE_SIGNING_KEY 必须是 64 位 hex(32 字节种子)');
  }
  const out = new Uint8Array(32);
  for (let i = 0; i < 32; i++) {
    out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function b64encode(bytes: Uint8Array): string {
  let bin = '';
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]!);
  return btoa(bin);
}

function nowSec(): number {
  return Math.floor(Date.now() / 1000);
}

function corsHeaders(): Record<string, string> {
  return {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
  };
}

function json(payload: unknown, status = 200): Response {
  return new Response(JSON.stringify(payload), {
    status,
    headers: {
      'Content-Type': 'application/json; charset=utf-8',
      'Cache-Control': 'no-store',
      ...corsHeaders(),
    },
  });
}
