"""PDF 转 Word/Excel(拍图转 Word、表格识别转 Excel)。

依赖 OCR 识别结果,第三阶段后续迭代实现:
- 拍图转 Word:OCR 版面分析 + python-docx 生成
- 表格识别转 Excel:表格结构还原 + openpyxl 生成
"""
