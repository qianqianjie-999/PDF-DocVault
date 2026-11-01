# 卡密兑换服务(Cloudflare Worker)

客户端内置 ed25519 **公钥**,本服务持有**私钥**。用户付款拿到卡密后,客户端把
「卡密 + 设备指纹」发到这里,这里验卡、签名,返回许可证。之后客户端完全离线,不再联网。

## 成本与前置

| 项 | 要求 |
|---|---|
| 账号 | Cloudflare 免费账号(Workers 免费额度 10 万次/天,足够) |
| 域名 | 用 `*.workers.dev` 子域即可,不强制备案域名 |
| 费用 | 0 元/月 |
| 依赖 | Node 18+、Wrangler CLI |

## 一、首次部署(一次性)

```bash
cd deploy/redeem-worker
npm install

# 1. 建卡密库,把返回的 id 填进 wrangler.toml 的 kv_namespaces.id
npx wrangler kv namespace create VOUCHERS      # 本地调试用
npx wrangler kv namespace create VOUCHERS --preview  # 可选

# 2. 登录
npx wrangler login

# 3. 写私钥(粘贴 .license/signing.key 的内容,64 位 hex)
npx wrangler secret put LICENSE_SIGNING_KEY

# 4. 部署
npx wrangler deploy
```

部署成功会打印地址,形如 `https://docvault-redeem.<你的子域>.workers.dev`。

## 二、把地址填进客户端

打包前设置环境变量(或直接改 `license_commands.rs` 里的默认值):

```bash
export PDF_DOCVAULT_REDEEM_URL="https://docvault-redeem.xxx.workers.dev"
pnpm tauri build
```

客户端的 `get_purchase_info` 也会用到地址,同一个变量控制。

## 三、发卡

```bash
# 在仓库根目录生成 100 张卡密
cargo run -p license-gen --release -- batch --n 100 --out codes-20261101.csv

# 灌进 KV(必须先等第一批卡密写入完成再做压测)
cd deploy/redeem-worker
npx wrangler kv bulk put ../../codes-20261101.csv.kv.json --binding VOUCHERS --remote
```

> KV 写入后全球生效有几十秒延迟,**导入完成后等 1 分钟再上架**。

卡密形如 `DV-SNK4Q-TX5BE-MF0KB-N586T`,把 CSV 上传到发卡平台(爱发电 / 面包多),
商品设为「永久授权卡密」,平台付款后会自动把卡密发给买家。

## 四、自测

```bash
URL="https://docvault-redeem.xxx.workers.dev"
DEVICE=$(python3 -c "import hashlib;print(hashlib.sha256(b'pdf-docvault::v1::test').hexdigest())")
CODE=$(sed -n '2p' ../../codes-20261101.csv | cut -d, -f1)

# 首次兑换
curl -s -X POST "$URL/redeem" -H 'Content-Type: application/json' \
  -d "{\"code\":\"$CODE\",\"device_id\":\"$DEVICE\"}" | head -c 300

# 换台设备(应返回 device_conflict)
curl -s -X POST "$URL/redeem" -H 'Content-Type: application/json' \
  -d "{\"code\":\"$CODE\",\"device_id\":\"$(printf 'a%.0s' {1..64})\"}"

# 换机(应成功,rebind_left 减 1)
curl -s -X POST "$URL/rebind" -H 'Content-Type: application/json' \
  -d "{\"code\":\"$CODE\",\"device_id\":\"$(printf 'b%.0s' {1..64})\"}"
```

拿返回的 `license` 到本机验签:

```bash
cargo run -p license-gen -- verify --blob "<license 字段>" --device "<device_id>"
```

## 五、运维

| 场景 | 操作 |
|---|---|
| 查某张卡是否已用 | `npx wrangler kv key get "voucher:XXXXX" --binding VOUCHERS --remote` |
| 用户换机次数用完 | 手工把记录里的 `rebind_count` 改回 0 |
| 用户卡密丢失 | 用 `license-gen sign --device <指纹>` 直接签一张许可证发给对方 |
| 调整换机上限 | 改 `wrangler.toml` 的 `MAX_REBIND` 后重新 deploy |
| 查看实时日志 | `npx wrangler tail` |

## 六、已知限制

1. **卡密即凭证**:谁先拿到卡密谁就能绑,这是发卡模式本身的特性,无法在技术上区分"本人"。
   缓解手段是卡密只在付款后自动发送,不要提前外流。
2. **KV 最终一致**:同一张卡密在同一秒内被两个设备并发兑换,理论上可能双绑。
   正常销售场景(一人一码)不会触发。
3. **重装系统会变设备指纹**(Linux 读 `/etc/machine-id`,Windows 读 MachineGuid)。
   所以必须保留换机通道,否则用户重装一次就锁死。
4. **私钥丢失 = 无法再发新授权**。老用户仍可用,但没法卖新单。请备份两份。
