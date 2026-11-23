"""从思源 SC 字体生成内核可精确匹配的中文字体。

wordcraft 内核按文档声明的字体名（如「宋体」「黑体」）与已装字体内部 family 名**精确匹配**，
不做别名/尾部匹配，因此需把思源字体的 name 表 family 改成「宋体」「黑体」再装。
用法：先按 docs/BUILD.md 下载 4 个思源 SC otf 放入 backend/static/fonts/，运行本脚本生成改名版。
OFL 协议允许改名再分发（新名不含保留名 Source Han 即可）。
"""
import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONTS = os.path.join(ROOT, "backend", "static", "fonts")

# (源文件, 中文 family, PostScript 名前缀, 输出文件)
JOBS = [
    ("SourceHanSerifSC-Regular.otf", "宋体", "SongTi", "SongTi-Regular.otf"),
    ("SourceHanSerifSC-Bold.otf", "宋体", "SongTi", "SongTi-Bold.otf"),
    ("SourceHanSansSC-Regular.otf", "黑体", "HeiTi", "HeiTi-Regular.otf"),
    ("SourceHanSansSC-Bold.otf", "黑体", "HeiTi", "HeiTi-Bold.otf"),
    # 等线是 Word 2016+ 中文默认字体，文档内嵌 EMF 图片里的文字常声明它，用黑体副本再改名覆盖
    ("HeiTi-Regular.otf", "等线", "DengXian", "DengXian-Regular.otf"),
    ("HeiTi-Bold.otf", "等线", "DengXian", "DengXian-Bold.otf"),
    # 微软雅黑是中文文档最常见声明字体之一，同样从黑体成品改名覆盖
    ("HeiTi-Regular.otf", "微软雅黑", "WeiRuanYaHei", "WeiRuanYaHei-Regular.otf"),
    ("HeiTi-Bold.otf", "微软雅黑", "WeiRuanYaHei", "WeiRuanYaHei-Bold.otf"),
    # Inter 是 deckcraft 主题默认字体（无中文字形且缺字不回退，画布中文显示方块），从黑体成品改名覆盖
    ("HeiTi-Regular.otf", "Inter", "InterCJK", "InterCJK-Regular.otf"),
    ("HeiTi-Bold.otf", "Inter", "InterCJK", "InterCJK-Bold.otf"),
]

RENAMED_IDS = {1, 2, 4, 6, 16, 17}  # family/subfamily/full/PS/typographic family/subfamily


def gbk_text() -> str:
    """GBK 全集字符（ASCII + 双字节区），覆盖中文几乎所有用字，含姓名生僻字。"""
    chars = {chr(c) for c in range(0x20, 0x7F)}
    for hi in range(0x81, 0xFF):
        for lo in range(0x40, 0x100):
            if lo == 0x7F:
                continue
            try:
                chars.add(bytes([hi, lo]).decode("gbk"))
            except UnicodeDecodeError:
                pass
    return "".join(sorted(chars))


def subset_gbk(path: str) -> None:
    """原地子集化为 GBK 全集：单字体 16-24MB 压到约 5MB，大幅缩短内核 font.add 解析耗时与安装清单体积。
    子集外生僻字（Unicode 扩展区等）缺字，由内核按缺字方块渲染。"""
    opts = subset.Options()
    opts.name_IDs = ["*"]
    opts.name_languages = ["*"]
    opts.notdef_outline = True
    font = subset.load_font(path, opts)
    subsetter = subset.Subsetter(opts)
    subsetter.populate(text=gbk_text())
    subsetter.subset(font)
    font.save(path)


def rename(src: str, family: str, ps_prefix: str, out: str) -> None:
    font = TTFont(os.path.join(FONTS, src))
    style = "Bold" if "Bold" in src else "Regular"
    name = font["name"]

    for nid in sorted(RENAMED_IDS):
        name.removeNames(nameID=nid)  # 删所有平台同名 ID 记录

    full = family if style == "Regular" else f"{family} {style}"
    ps = f"{ps_prefix}-{style}"
    for nid, value in ((1, family), (2, style), (4, full), (6, ps), (16, family), (17, style)):
        # 仅写 Windows 平台 UTF-16 记录（Mac Roman 无法编码中文）
        name.setName(value, nid, 3, 1, 0x409)

    if "CFF " in font:  # OTF 的 CFF 表也带内部名，一并改
        font["CFF "].cff.fontNames = [ps]

    font.save(os.path.join(FONTS, out))
    subset_gbk(os.path.join(FONTS, out))
    print(f"{src} -> {out}  family={family} style={style}")


def main() -> int:
    jobs = [job for job in JOBS if not os.path.exists(os.path.join(FONTS, job[3]))]
    if not jobs:
        print("输出字体均已存在，无需生成")
        return 0
    missing = [src for src, *_ in jobs if not os.path.exists(os.path.join(FONTS, src))]
    if missing:
        print("缺少源字体（先按 docs/BUILD.md 下载）:", ", ".join(missing))
        return 1
    for job in jobs:
        rename(*job)
    return 0


if __name__ == "__main__":
    sys.exit(main())
