"""Contact sheet of a folder of screenshots (e.g. the user-map smoke test's out/usermaps/shots).
usage: python tools/checks/contact_sheet.py <shots dir> <out.png> [cols]"""
import os, sys
from PIL import Image, ImageDraw

shots, out = sys.argv[1], sys.argv[2]
cols = int(sys.argv[3]) if len(sys.argv) > 3 else 6
files = sorted(f for f in os.listdir(shots) if f.endswith('.png'))
W, H = 320, 240
rows = (len(files) + cols - 1) // cols
sheet = Image.new('RGB', (cols * W, rows * (H + 14)), (40, 40, 40))
d = ImageDraw.Draw(sheet)
for i, f in enumerate(files):
    x, y = i % cols * W, i // cols * (H + 14)
    try:
        im = Image.open(os.path.join(shots, f)).convert('RGB').resize((W, H))
        sheet.paste(im, (x, y + 14))
    except Exception as e:
        d.text((x + 4, y + 40), str(e)[:40], fill=(255, 80, 80))
    d.text((x + 4, y + 1), f[:-4], fill=(255, 255, 255))
sheet.save(out)
print(out, len(files))
