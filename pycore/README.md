# pycore - Python AI/OCR 服务

> 本阶段为占位,规划书第三阶段(OCR 与扫描)实现。

## 规划内容

依据规划书 v2.0,本模块将在后续阶段实现:

### OCR 文字识别(PaddleOCR)
- 提取文字:从图片/扫描件识别可编辑文字
- 拍图转 Word:拍照后生成可编辑 Word 文档
- 表格识别:图片表格转 Excel
- 多语言:中/英/日/韩
- 版面分析:标题/正文/表格/图片区域

### 文档扫描(OpenCV + Photon)
- 手机照片变扫描文档:边缘检测、透视矫正、去阴影、增强
- 批量扫描:多张照片合成 PDF
- 证件扫描:身份证/护照正反面排版
- 试卷去手写:Inpaint 图像修复

### Rust-Python 协作(进程级隔离)
- 简单任务纯 Rust 处理
- 复杂任务 Rust spawn 独立 Python 进程,通过文件/stdout 通信
- 避免 GIL,Python 崩溃不影响主程序

## 依赖(规划)

```
paddleocr==2.9.x
paddlepaddle==3.0.x
PyMuPDF==1.24.x
opencv-python==4.10.x
Pillow==10.4.x
numpy==2.1.x
```

## 目录结构(规划)

```
pycore/
├── ocr/           OCR 识别封装
├── scan/          文档扫描增强
├── convert/       PDF 转 Word/Excel
├── runtime/       Python 进程隔离调用框架
└── requirements.txt
```
