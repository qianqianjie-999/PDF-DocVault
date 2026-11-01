//! PDF-DocVault 许可证签发工具
//!
//! 私钥只存在于本机 `.license/signing.key`,**绝不出本机**。
//! 客户端内置的只有公钥,因此即使客户端被逆向也伪造不出许可证。
//!
//! 常用命令:
//!   license-gen keygen --out-dir .license      生成密钥对(只做一次)
//!   license-gen batch --n 100 --out codes.csv  批量生成卡密
//!   license-gen sign --device <id>             离线手工签发(卡密系统故障时的兜底)
//!   license-gen pubkey                         查看当前二进制内置的公钥
//!   license-gen verify --blob <blob> --device <id>  本地自检

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ed25519_dalek::SigningKey;
use pdf_docvault_core::activation::license;
use pdf_docvault_core::activation::voucher;
use rand::rngs::OsRng;

const DEFAULT_KEY_DIR: &str = ".license";
const SIGNING_KEY_FILE: &str = "signing.key";
const PUBKEY_FILE: &str = "pubkey.hex";
const DEFAULT_PLAN: &str = "full";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "help" || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return ExitCode::SUCCESS;
    }

    let cmd = args[0].as_str();
    let rest = &args[1..];
    let result = match cmd {
        "keygen" => cmd_keygen(rest),
        "batch" => cmd_batch(rest),
        "sign" => cmd_sign(rest),
        "pubkey" => cmd_pubkey(),
        "verify" => cmd_verify(rest),
        other => Err(format!(
            "未知命令: {other}\n运行 `license-gen help` 查看用法"
        )),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("错误: {e}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        r#"PDF-DocVault 许可证签发工具

用法:
  license-gen keygen [--out-dir <dir>] [--force]
      生成 ed25519 密钥对。私钥写入 <dir>/signing.key(权限 0600),
      公钥写入 <dir>/pubkey.hex。**只需执行一次**,私钥务必备份。

  license-gen batch --n <数量> [--out <csv路径>] [--batch <批次名>]
      批量生成卡密。同时产出 <csv>.kv.json,供 wrangler 批量导入 KV:
        npx wrangler kv bulk put <csv>.kv.json --binding VOUCHERS --remote

  license-gen sign --device <设备指纹> [--plan full] [--expiry 0] [--out <文件>]
      离线手工签发许可证。expiry=0 表示永久。
      这是卡密系统故障时的人工兜底通道。

  license-gen pubkey
      打印当前二进制内置的公钥,用于确认 build.rs 注入是否生效。

  license-gen verify --blob <许可证> --device <设备指纹>
      本地验签自检,确认签出来的许可证能被客户端认可。

私钥查找顺序: --key <path>  >  环境变量 PDF_DOCVAULT_LICENSE_KEY(hex)  >  .license/signing.key
"#
    );
}

// ---------------------------------------------------------------- keygen

fn cmd_keygen(args: &[String]) -> Result<(), String> {
    let out_dir =
        PathBuf::from(opt_value(args, "--out-dir").unwrap_or_else(|| DEFAULT_KEY_DIR.into()));
    let force = args.iter().any(|a| a == "--force");

    let key_path = out_dir.join(SIGNING_KEY_FILE);
    let pub_path = out_dir.join(PUBKEY_FILE);

    if key_path.exists() && !force {
        return Err(format!(
            "{} 已存在。\n\
             换新密钥会让**所有已售出的许可证作废** —— 只有确认要这么做时才加 --force。\n\
             如果只是想看当前公钥,用 `license-gen pubkey`。",
            key_path.display()
        ));
    }

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建目录失败: {e}"))?;

    let signing = SigningKey::generate(&mut OsRng);
    let seed = signing.to_bytes();
    let pubkey = signing.verifying_key().to_bytes();

    std::fs::write(&key_path, format!("{}\n", hex::encode(seed)))
        .map_err(|e| format!("写私钥失败: {e}"))?;
    restrict_permissions(&key_path)?;
    std::fs::write(&pub_path, format!("{}\n", hex::encode(pubkey)))
        .map_err(|e| format!("写公钥失败: {e}"))?;

    println!();
    println!("  密钥对已生成");
    println!("  ─────────────────────────────────────────────");
    println!("  私钥: {}", key_path.display());
    println!("  公钥: {}", pub_path.display());
    println!("  公钥 hex: {}", hex::encode(pubkey));
    println!("  ─────────────────────────────────────────────");
    println!("  ⚠ 私钥丢了 = 无法再给新用户签发授权;私钥泄露 = 谁都能伪造授权。");
    println!(
        "  ⚠ 请立刻把 {} 备份到密码管理器 + 离线介质。",
        key_path.display()
    );
    println!("  ⚠ 换密钥会让所有已售许可证当场失效,不要轻易重跑 keygen。");
    println!();
    println!("  下一步:填进兑换服务(Wrangler secret)");
    println!("    npx wrangler secret put LICENSE_SIGNING_KEY   # 粘贴上面的私钥 hex");
    println!("  然后重新构建客户端,公钥会被 build.rs 自动读取。");
    println!();
    Ok(())
}

fn restrict_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| format!("读取私钥权限失败: {e}"))?
            .permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms).map_err(|e| format!("设置私钥权限失败: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

// ---------------------------------------------------------------- batch

fn cmd_batch(args: &[String]) -> Result<(), String> {
    let n: usize = opt_value(args, "--n")
        .ok_or_else(|| "缺少 --n <数量>".to_string())?
        .parse()
        .map_err(|_| "--n 必须是正整数".to_string())?;
    if n == 0 || n > 100_000 {
        return Err("--n 取值范围 1 - 100000".into());
    }

    let out = PathBuf::from(opt_value(args, "--out").unwrap_or_else(|| "codes.csv".into()));
    let batch_name = opt_value(args, "--batch")
        .unwrap_or_else(|| chrono::Local::now().format("%Y%m%d-%H%M%S").to_string());
    let created_at = chrono::Local::now().to_rfc3339();

    let mut codes = Vec::with_capacity(n);
    let mut seen: HashSet<String> = HashSet::with_capacity(n);
    while codes.len() < n {
        let code = voucher::generate();
        if seen.insert(voucher::normalize(&code)) {
            codes.push(code);
        }
    }

    let mut csv = String::from("code,batch,created_at,status\n");
    for code in &codes {
        csv.push_str(&format!("{code},{batch_name},{created_at},unused\n"));
    }
    std::fs::write(&out, csv).map_err(|e| format!("写 CSV 失败: {e}"))?;

    // wrangler kv bulk put 需要的格式:[{ "key": "...", "value": "..." }]
    let kv_path = out.with_extension("kv.json");
    let kv: Vec<String> = codes
        .iter()
        .map(|code| {
            format!(
                "  {{\"key\":\"{}\",\"value\":\"{}\"}}",
                voucher::storage_key(code),
                "{\\\"status\\\":\\\"unused\\\"}"
            )
        })
        .collect();
    let kv_json = format!("[\n{}\n]\n", kv.join(",\n"));
    std::fs::write(&kv_path, kv_json).map_err(|e| format!("写 KV JSON 失败: {e}"))?;

    println!();
    println!("  已生成 {n} 个卡密");
    println!("  ─────────────────────────────────────────────");
    println!("  存档 CSV : {}", out.display());
    println!("  KV 导入  : {}", kv_path.display());
    println!("  批次     : {batch_name}");
    println!(
        "  样例     : {}",
        codes.first().cloned().unwrap_or_default()
    );
    println!("  ─────────────────────────────────────────────");
    println!("  导入兑换服务:");
    println!(
        "    npx wrangler kv bulk put {} --binding VOUCHERS --remote",
        kv_path.display()
    );
    println!();
    println!("  ⚠ CSV 与 kv.json 就是钱,别提交到仓库、别随手发人。");
    println!();
    Ok(())
}

// ---------------------------------------------------------------- sign

fn cmd_sign(args: &[String]) -> Result<(), String> {
    let device =
        opt_value(args, "--device").ok_or_else(|| "缺少 --device <设备指纹>".to_string())?;
    let plan = opt_value(args, "--plan").unwrap_or_else(|| DEFAULT_PLAN.into());
    let expiry: i64 = opt_value(args, "--expiry")
        .map(|s| {
            s.parse()
                .map_err(|_| "--expiry 必须是时间戳(0 表示永久)".to_string())
        })
        .transpose()?
        .unwrap_or(0);

    let seed = load_signing_key(opt_value(args, "--key"))?;
    let blob = license::sign_license(&seed, &device, &plan, expiry)
        .map_err(|e| format!("签发失败: {e}"))?;

    // 自检:签出来的东西必须能被客户端公钥验过
    let parsed = license::License::from_blob(&blob).map_err(|e| format!("自检失败: {e}"))?;
    parsed
        .verify(&device)
        .map_err(|e| format!("自检失败(签出来了但验不过): {e}"))?;

    match opt_value(args, "--out") {
        Some(path) => {
            std::fs::write(&path, &blob).map_err(|e| format!("写文件失败: {e}"))?;
            println!("  已签发并自检通过 → {path}");
        }
        None => println!("{blob}"),
    }
    Ok(())
}

fn load_signing_key(explicit: Option<String>) -> Result<[u8; 32], String> {
    if let Some(path) = explicit {
        return read_key_file(Path::new(&path));
    }
    if let Ok(hex_str) = std::env::var("PDF_DOCVAULT_LICENSE_KEY") {
        return voucher::key_from_hex(&hex_str);
    }
    let default = Path::new(DEFAULT_KEY_DIR).join(SIGNING_KEY_FILE);
    if default.exists() {
        return read_key_file(&default);
    }
    Err(format!(
        "找不到签名私钥。请先运行 `license-gen keygen --out-dir {DEFAULT_KEY_DIR}`,\
         或用 --key <path> / 环境变量 PDF_DOCVAULT_LICENSE_KEY 指定"
    ))
}

fn read_key_file(path: &Path) -> Result<[u8; 32], String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取私钥 {} 失败: {e}", path.display()))?;
    voucher::key_from_hex(&content)
}

// ---------------------------------------------------------------- pubkey / verify

fn cmd_pubkey() -> Result<(), String> {
    let hex_str = license::public_key_hex();
    let injected = license::is_production_key();
    println!("  内置公钥: {hex_str}");
    if injected {
        println!("  来源    : 构建期注入(可用于正式发布)");
    } else {
        println!("  来源    : 开发种子派生(**仅供本地联调,不能发布**)");
        println!("  修复    : cargo run -p license-gen -- keygen --out-dir .license 后重新构建");
    }
    Ok(())
}

fn cmd_verify(args: &[String]) -> Result<(), String> {
    let blob = opt_value(args, "--blob").ok_or_else(|| "缺少 --blob <许可证>".to_string())?;
    let device =
        opt_value(args, "--device").ok_or_else(|| "缺少 --device <设备指纹>".to_string())?;
    let lic = license::License::from_blob(&blob).map_err(|e| format!("解析失败: {e}"))?;
    lic.verify(&device).map_err(|e| format!("验签失败: {e}"))?;
    println!("  验签通过");
    println!("  设备    : {}", lic.device_id);
    println!("  套餐    : {}", lic.plan);
    println!(
        "  期限    : {}",
        if lic.is_perpetual() {
            "永久".to_string()
        } else {
            chrono::DateTime::from_timestamp(lic.expiry, 0)
                .map(|d| d.with_timezone(&chrono::Local).to_rfc3339())
                .unwrap_or_else(|| lic.expiry.to_string())
        }
    );
    Ok(())
}

// ---------------------------------------------------------------- 参数解析

fn opt_value(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}
