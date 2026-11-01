//! 卡密(兑换码)编解码。
//!
//! 卡密是**无含义的随机凭证**,不携带任何权限信息 —— 权限由服务端在兑换时
//! 用私钥签名成 license 决定。卡密本身只起到"谁付过钱"的凭据作用。
//!
//! 结构:80 bit 随机 payload + 20 bit 校验和 = 100 bit,Base32(Crockford)编码成 20 字符,
//! 展示时按 5 字符一组分为 4 段:`DV-XXXXX-XXXXX-XXXXX-XXXXX`。
//!
//! 校验和的作用不是防伪(防伪靠服务端查库),而是让客户端在联网前就能拦掉
//! 手抄错、漏字符的输入,避免用户拿到"卡密无效"却不知道是自己打错了。

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Crockford Base32:去掉 I / L / O / U,避免手抄歧义
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// 随机 payload 字节数(80 bit)
const PAYLOAD_BYTES: usize = 10;
/// payload 编码后的字符数(80 / 5 = 16)
const PAYLOAD_CHARS: usize = 16;
/// 校验和字符数(20 bit)
const CHECK_CHARS: usize = 4;
/// 卡密总字符数(不含分隔符)
pub const CODE_CHARS: usize = PAYLOAD_CHARS + CHECK_CHARS;
/// 展示前缀
pub const PREFIX: &str = "DV";

/// 生成一个新卡密,返回展示格式 `DV-XXXXX-XXXXX-XXXXX-XXXXX`。
pub fn generate() -> String {
    let mut payload = [0u8; PAYLOAD_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut payload);
    encode_payload(&payload)
}

/// 把 10 字节 payload 编码成完整展示格式卡密。
pub fn encode_payload(payload: &[u8; PAYLOAD_BYTES]) -> String {
    let body = encode_base32(payload);
    let check = checksum_chars(payload);
    pretty(&format!("{body}{check}"))
}

/// 归一化用户输入:去分隔符、转大写、纠正易混字符。
///
/// 返回去掉分隔符的 20 字符(不保证校验通过),用于 `is_valid_format` 判断。
pub fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(CODE_CHARS);
    for ch in input.chars() {
        if !ch.is_ascii_alphanumeric() {
            continue; // 空格、连字符、零宽字符一律丢弃
        }
        let c = ch.to_ascii_uppercase();
        // Crockford 的易混字符归一
        let c = match c {
            'O' => '0',
            'I' | 'L' => '1',
            'U' => 'V',
            _ => c,
        };
        out.push(c);
    }
    // 用户可能把前缀(DV)连卡密一起粘贴,甚至重复粘贴一次。
    // 只在长度超出时才剥离,避免误伤本身以 DV 开头的载荷。
    while out.len() > CODE_CHARS && out.starts_with(PREFIX) {
        out.drain(..PREFIX.len());
    }
    out
}

/// 校验格式(长度 + 字符集 + 校验和)。用于联网前的本地预检。
pub fn is_valid_format(input: &str) -> bool {
    let code = normalize(input);
    if code.len() != CODE_CHARS {
        return false;
    }
    let (body, check) = code.split_at(PAYLOAD_CHARS);
    let Some(payload) = decode_base32(body) else {
        return false;
    };
    let Ok(payload): Result<[u8; PAYLOAD_BYTES], _> = payload.try_into() else {
        return false;
    };
    checksum_chars(&payload) == check
}

/// 判断某个字符是否是合法卡密字符(供前端逐字符过滤)
pub fn is_code_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

fn pretty(code: &str) -> String {
    let mut s = String::with_capacity(PREFIX.len() + CODE_CHARS + 5);
    s.push_str(PREFIX);
    for (i, chunk) in code.as_bytes().chunks(5).enumerate() {
        let _ = i;
        s.push('-');
        s.push_str(std::str::from_utf8(chunk).unwrap_or_default());
    }
    s
}

fn encode_base32(data: &[u8]) -> String {
    let mut out = String::with_capacity(PAYLOAD_CHARS);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buf >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buf << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

fn decode_base32(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(PAYLOAD_BYTES);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for ch in s.bytes() {
        let idx = ALPHABET.iter().position(|&a| a == ch)? as u32;
        buf = (buf << 5) | idx;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn checksum_chars(payload: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"pdf-docvault-voucher-v1");
    h.update(payload);
    let d = h.finalize();
    // 取前 20 bit
    let v = ((d[0] as u32) << 12) | ((d[1] as u32) << 4) | ((d[2] as u32) >> 4);
    let mut s = String::with_capacity(CHECK_CHARS);
    for i in (0..CHECK_CHARS).rev() {
        s.push(ALPHABET[((v >> (i * 5)) & 0x1f) as usize] as char);
    }
    s
}

/// 把卡密转为服务端可索引的固定键(避免大小写/分隔符差异导致查不到)
pub fn storage_key(input: &str) -> String {
    format!("voucher:{code}", code = normalize(input))
}

/// 把 32 字节签名密钥编码成可放进环境变量的字符串(hex)
pub fn key_to_hex(seed: &[u8; 32]) -> String {
    hex::encode(seed)
}

/// 从 hex 解析签名密钥
pub fn key_from_hex(hex_str: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_str.trim()).map_err(|e| format!("签名密钥不是合法 hex: {e}"))?;
    bytes
        .try_into()
        .map_err(|v: Vec<u8>| format!("签名密钥应为 32 字节,实际 {} 字节", v.len()))
}

/// base64url 无填充编码(供 Worker/CLI 交换裸签名时使用)
pub fn b64url(data: &[u8]) -> String {
    B64URL.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_is_valid() {
        for _ in 0..200 {
            let code = generate();
            assert!(is_valid_format(&code), "生成的卡密应自校验通过: {code}");
            assert_eq!(normalize(&code).len(), CODE_CHARS);
        }
    }

    #[test]
    fn generated_codes_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..500 {
            assert!(seen.insert(generate()), "80bit 随机空间出现重复,检查随机源");
        }
    }

    #[test]
    fn format_shape() {
        let code = generate();
        let parts: Vec<&str> = code.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0], PREFIX);
        for p in &parts[1..] {
            assert_eq!(p.len(), 5);
        }
    }

    #[test]
    fn normalize_accepts_sloppy_input() {
        let code = generate();
        // 小写 + 空格 + 无分隔符
        let sloppy = normalize(&code).to_lowercase();
        let messy = sloppy
            .as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(normalize(&messy), normalize(&code));
        assert!(is_valid_format(&messy));
    }

    #[test]
    fn normalize_fixes_confusable_chars() {
        // O->0, I/L->1
        assert_eq!(normalize("OIL"), "011");
        assert_eq!(normalize("dv-ab cd"), "DVABCD");
    }

    #[test]
    fn prefix_is_stripped() {
        let code = generate();
        let with_prefix_doubled = format!("DV{}", code.replace('-', ""));
        // 用户把前缀重复粘进来时仍应解出同一张卡
        assert_eq!(normalize(&with_prefix_doubled), normalize(&code));
    }

    #[test]
    fn rejects_typo() {
        let code = generate();
        let mut chars: Vec<char> = normalize(&code).chars().collect();
        // 改动第 3 位
        chars[2] = if chars[2] == 'A' { 'B' } else { 'A' };
        let typo: String = chars.into_iter().collect();
        assert!(!is_valid_format(&typo), "单字符改错必须被校验和拦下");
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(!is_valid_format(""));
        assert!(!is_valid_format("DV-ABCDE-FGHJK"));
        assert!(!is_valid_format("这显然不是卡密"));
    }

    #[test]
    fn rejects_non_alphabet_chars() {
        // I / L / O / U 不在字母表,归一后可能合法,但直接构造非法字符应失败
        assert!(!is_valid_format("IIIIIIIIIIIIIIIIIIII"));
    }

    #[test]
    fn key_hex_roundtrip() {
        let seed = [7u8; 32];
        let h = key_to_hex(&seed);
        assert_eq!(key_from_hex(&h).unwrap(), seed);
        assert!(key_from_hex("zz").is_err());
        assert!(key_from_hex("0011").is_err());
    }

    #[test]
    fn storage_key_is_case_insensitive() {
        let code = generate();
        assert_eq!(storage_key(&code), storage_key(&code.to_lowercase()));
        // 键里只保留 20 字符载荷(不含 DV 前缀与分隔符),长度固定
        assert_eq!(storage_key(&code).len(), "voucher:".len() + CODE_CHARS);
        assert!(storage_key(&code).starts_with("voucher:"));
    }
}
