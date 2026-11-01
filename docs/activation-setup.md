# 激活系统上线清单

定价:**1 元,一次性买断,永久授权**。不续费、不分级。

## 一、链路总览

```
用户付款 ──> 发卡平台自动发卡密 ──> 客户端输卡密
                                      │
                                      ├─ 卡密+设备指纹 → 兑换 Worker(Cloudflare)
                                      │                    ├─ 查 KV:未用 → 绑定本机
                                      │                    └─ ed25519 私钥签名
                                      │                 ← 许可证 blob
                                      └─ 内置公钥验签 → 落库 → 之后完全离线
```

客户端**只有公钥**。就算有人把安装包逆向、把源码全改了,也签不出一张能通过验签的许可证。

## 二、已经做完的

| 模块 | 位置 | 说明 |
|---|---|---|
| 卡密编解码 | `core/src/activation/voucher.rs` | 80bit 随机 + 20bit 校验,`DV-XXXXX-XXXXX-XXXXX-XXXXX` |
| 公钥注入 | `core/build.rs` | release 缺公钥会有明确告警;debug 回落开发种子 |
| 打包强拦截 | `frontend/src-tauri/build.rs` | release 无正式公钥 → **直接中止打包** |
| 签发 CLI | `tools/license-gen/` | keygen / batch / sign / pubkey / verify |
| 兑换服务 | `deploy/redeem-worker/` | Cloudflare Worker + KV,含 19 项 e2e 断言 |
| 客户端兑换 | `commands/license_commands.rs` | 超时 20s,返回的许可证**本地再验一遍** |
| 换机 | 同上 | 默认 3 次,超过提示联系卖家 |
| 激活页 | `views/desktop/ActivationView.vue` | 卡密输入自动格式化 + 购买引导 + 离线兜底 |

## 三、上线前必做(3 件事)

### 1. 备份私钥(今天就要做)

`.license/signing.key` 已经生成好了,**这是整套系统唯一的私钥**。

```bash
cat .license/signing.key     # 64 位 hex,立即存进密码管理器 + 离线介质各一份
```

| 丢了的后果 | 泄露的后果 |
|---|---|
| 老用户还能用,但**再也发不了新授权**,等于卖不了 | 谁拿到都能伪造授权,软件白送 |

换密钥 = 已售许可证全部作废。除非确认要清零重来,否则永远不要重跑 `keygen`。

### 2. 部署兑换服务

见 `deploy/redeem-worker/README.md`,四条命令:

```bash
cd deploy/redeem-worker && npm install
npx wrangler kv namespace create VOUCHERS   # 把 id 填进 wrangler.toml
npx wrangler secret put LICENSE_SIGNING_KEY # 粘 .license/signing.key
npx wrangler deploy
```

拿到地址后,打包时指定:

```bash
export PDF_DOCVAULT_REDEEM_URL="https://docvault-redeem.xxx.workers.dev"
pnpm tauri build
```

### 3. 上架发卡

```bash
cargo run -p license-gen --release -- batch --n 100 --out codes-20261101.csv
cd deploy/redeem-worker
npx wrangler kv bulk put ../../codes-20261101.csv.kv.json --binding VOUCHERS --remote
```

把 CSV 传进发卡平台(爱发电 / 面包多),商品名「永久授权卡密」,价格 1 元,
设置「付款后自动发货」。**导入后等 1 分钟再开卖**(KV 有传播延迟)。

## 四、还需要你提供的两样东西

| 项 | 放哪 | 用途 |
|---|---|---|
| 微信收款码图片 | `frontend/public/pay-qr.png` | 激活页展示;不放进也能用,只是显示文字引导 |
| 店铺链接 | `PDF_DOCVAULT_PURCHASE_URL` 环境变量,或改 `core/src/activation/purchase.rs` 的默认值 | 「前往购买卡密」按钮跳转 |

## 五、常见问题

**用户重装系统后授权失效?**
设备指纹来自 `machine-id`(Linux)/ MachineGuid(Windows),重装会变。
让用户在激活页输入原卡密,会提示是否把授权转移到本机,消耗 1 次换机机会(共 3 次)。
3 次用完:运维里把 KV 记录的 `rebind_count` 改回 0,或直接
`license-gen sign --device <新指纹>` 手工签一张。

**私钥丢了怎么办?**
老用户不受影响(验签只用公钥),但新用户发不了卡。
应急:临时用 `license-gen sign` 手工签发,同时尽快做一次密钥轮换计划
(轮换需要发布新版本客户端,老版本只认老公钥)。

**能防住"一码多机"吗?**
绑定 + 换机次数已经挡住了正常滥用。彻底防住需要每次启动联网校验,
这与"本地优先、数据不外传"的产品定位冲突,不建议做。

**卡密被猜到吗?**
80bit 随机空间(约 1.2×10^24),配合格式校验和,暴力猜不可行。
