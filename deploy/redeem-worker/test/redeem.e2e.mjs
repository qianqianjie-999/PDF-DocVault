/**
 * 兑换服务端到端测试
 *
 * 用 Miniflare 在本地起一个真实的 Worker 运行时(含 KV),走完
 * 兑换 → 冲突 → 换机 → 换机次数耗尽 的全流程,
 * 并把签出来的许可证交给 Rust 客户端验签,确认两端协议一致。
 *
 * 运行: npm test
 * 依赖: 需要仓库根 .license/signing.key,以及一个真实卡密(见 --code 或环境变量)
 */

import { Miniflare } from 'miniflare';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '../../..');

const seed = fs.readFileSync(path.join(repoRoot, '.license/signing.key'), 'utf8').trim();
const codeFile = process.env.TEST_CODE_FILE;
if (!codeFile) {
  console.error('缺少 TEST_CODE_FILE=<卡密CSV路径>');
  process.exit(2);
}
const codes = fs
  .readFileSync(codeFile, 'utf8')
  .split('\n')
  .slice(1)
  .map((l) => l.split(',')[0]?.trim())
  .filter(Boolean);
assert.ok(codes.length >= 2, '至少需要 2 张测试卡密');

const A = 'a'.repeat(64);
const B = 'b'.repeat(64);
const C = 'c'.repeat(64);
const D = 'd'.repeat(64);
const E = 'e'.repeat(64);

let passed = 0;
function check(name, cond, extra = '') {
  if (cond) {
    passed++;
    console.log(`  ✓ ${name}`);
  } else {
    console.error(`  ✗ ${name} ${extra}`);
    process.exitCode = 1;
  }
}

const mf = new Miniflare({
  modules: true,
  scriptPath: path.join(here, '../.tmp/index.mjs'),
  compatibilityDate: '2024-11-01',
  kvNamespaces: ['VOUCHERS'],
  bindings: {
    LICENSE_SIGNING_KEY: seed,
    MAX_REBIND: '3',
    PLAN: 'full',
    LICENSE_TTL: '0',
  },
});

async function post(p, body) {
  const res = await mf.dispatchFetch(`http://redeem.test${p}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  const text = await res.text();
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {
    /* 保留原文用于断言 */
  }
  return { status: res.status, json, text };
}

try {
  // 预置卡密(键必须与 Worker 的 normalizeCode 结果一致,否则查不到)
  const kv = await mf.getKVNamespace('VOUCHERS');
  const norm = (raw) => {
    let s = raw
      .toUpperCase()
      .replace(/[^0-9A-Z]/g, '')
      .replace(/O/g, '0')
      .replace(/[IL]/g, '1')
      .replace(/U/g, 'V');
    while (s.length > 20 && s.startsWith('DV')) s = s.slice(2);
    return s;
  };
  for (const c of codes.slice(0, 2)) {
    await kv.put(`voucher:${norm(c)}`, JSON.stringify({ status: 'unused' }));
  }
  const [code1, code2] = codes;

  console.log('\n[1] 基础校验');
  const health = await mf.dispatchFetch('http://redeem.test/health');
  check('GET /health 返回 200', health.status === 200);

  let r = await post('/redeem', { code: code1, device_id: A });
  check('首次兑换成功', r.status === 200, r.text);
  check('返回许可证且标记为新绑定', Boolean(r.json?.license) && r.json.newly_bound === true);
  check('永久授权 expiry=0', r.json?.expiry === 0 && r.json?.perpetual === true);
  check('卡密尾号回传', r.json?.code_suffix === code1.slice(-4));

  const license1 = r.json.license;

  // 交给 Rust 客户端验签 —— 这是最关键的一致性检查
  console.log('\n[2] 跨语言验签(Rust 客户端)');
  const bin = path.join(repoRoot, 'target/debug/license-gen');
  const verifyOut = execFileSync(bin, ['verify', '--blob', license1, '--device', A], {
    encoding: 'utf8',
  });
  check('Rust 端验签通过', verifyOut.includes('验签通过'), verifyOut);

  console.log('\n[3] 幂等与冲突');
  r = await post('/redeem', { code: code1, device_id: A });
  check('同设备重复兑换幂等成功', r.status === 200 && r.json.newly_bound === false, r.text);

  r = await post('/redeem', { code: code1, device_id: B });
  check('换设备兑换返回 409', r.status === 409, r.text);
  check('错误码为 device_conflict', r.json?.error === 'device_conflict');

  console.log('\n[4] 归一化');
  const messy = code2.toLowerCase().replace(/-/g, ' ');
  r = await post('/redeem', { code: messy, device_id: A });
  check('小写+空格卡密可正常兑换', r.status === 200, r.text);

  console.log('\n[5] 换机上限');
  r = await post('/rebind', { code: code1, device_id: B });
  check('换机到 B 成功', r.status === 200 && r.json.rebind_left === 2, r.text);

  r = await post('/rebind', { code: code1, device_id: C });
  check('换机到 C 成功', r.status === 200 && r.json.rebind_left === 1, r.text);

  r = await post('/rebind', { code: code1, device_id: D });
  check('换机到 D 成功', r.status === 200 && r.json.rebind_left === 0, r.text);

  r = await post('/rebind', { code: code1, device_id: E });
  check('第 4 次换机被拒绝 429', r.status === 429, r.text);
  check('错误码为 rebind_exhausted', r.json?.error === 'rebind_exhausted');

  console.log('\n[6] 异常输入');
  r = await post('/redeem', { code: 'DV-AAAAA-AAAAA-AAAAA-AAAAA', device_id: A });
  check('不存在的卡密返回 404', r.status === 404 && r.json?.error === 'voucher_not_found', r.text);

  r = await post('/redeem', { code: 'too-short', device_id: A });
  check('格式错误返回 400', r.status === 400 && r.json?.error === 'invalid_code_format');

  r = await post('/redeem', { code: code1, device_id: 'not-a-device' });
  check('设备指纹非法返回 400', r.status === 400 && r.json?.error === 'invalid_device');

  r = await post('/unknown', { code: code1, device_id: A });
  check('未知接口返回 404', r.status === 404);

  console.log(`\n通过 ${passed} 项断言\n`);
} finally {
  await mf.dispose();
}
