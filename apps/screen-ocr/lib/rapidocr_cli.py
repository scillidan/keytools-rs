# /// script
# requires-python = ">=3.8"
# dependencies = ["rapidocr-onnxruntime"]
# ///
import argparse
import os
import sys
from pathlib import Path


PP_OCR_V5_LANGS = sorted([
    "ch", "chinese_cht", "en", "japan", "korean",
    "latin", "cyrillic", "arabic", "devanagari",
    "ta", "te", "el", "th", "eslav",
])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("image_path", nargs="?", default="")
    parser.add_argument("output_path", nargs="?", default="")
    parser.add_argument("lang", nargs="?", default="ch")
    parser.add_argument("--use-angle-cls", action="store_true")
    parser.add_argument("--list-langs", action="store_true", help="List PP-OCRv5 supported languages")
    args = parser.parse_args()

    if args.list_langs:
        for code in PP_OCR_V5_LANGS:
            print(code)
        sys.exit(0)

    if not args.image_path or not args.output_path:
        print("Usage: ocr_cli.py <image_path> <output_path> [lang] [--use-angle-cls]", file=sys.stderr)
        sys.exit(1)

    from rapidocr_onnxruntime import RapidOCR

    img_path = args.image_path
    output_path = args.output_path

    if os.path.isdir(output_path):
        stem = Path(img_path).stem
        output_path = os.path.join(output_path, stem + ".txt")

    engine = RapidOCR(
        lang=args.lang,
        use_angle_cls=args.use_angle_cls,
    )
    result, _ = engine(img_path)

    with open(output_path, "w", encoding="utf-8") as f:
        if result:
            for line in result:
                f.write(line[1] + "\n")

    print(output_path)


if __name__ == "__main__":
    main()
