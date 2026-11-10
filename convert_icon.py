from pathlib import Path
from PIL import Image

source = Path(__file__).with_name("PDFool.png")
with Image.open(source) as image:
    image.convert("RGBA").save(source.with_suffix(".ico"), format="ICO", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
