//! 临时探针:对真实 PDF 按各档位压缩,输出体积/耗时对比。
//!
//! 用法:cargo run -p pdf_docvault_core --example compress_probe -- <file.pdf>

use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("需要传入 PDF 路径");
    let input = Path::new(&input);
    let dir = std::env::var("HOME").unwrap_or_else(|_| ".".into()) + "/tmp";
    std::fs::create_dir_all(&dir).unwrap();
    let base = input.file_stem().unwrap().to_string_lossy().to_string();
    let orig = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    println!(
        "输入: {}  {} bytes ({:.2} MB)",
        input.display(),
        orig,
        orig as f64 / 1048576.0
    );

    for level in [1u8, 4, 6, 9] {
        let out = Path::new(&dir).join(format!("{base}-L{level}.pdf"));
        match pdf_docvault_core::pdf::compress(input, &out, level) {
            Ok(s) => println!(
                "L{level}: {:>9} bytes  节省 {:>6.1}%  重编码图 {} 张  耗时 {} ms  回退={}",
                s.compressed_size, s.ratio, s.images_reencoded, s.elapsed_ms, s.fell_back
            ),
            Err(e) => println!("L{level}: 失败 {e}"),
        }
    }
}
