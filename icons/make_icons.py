# The icons: pixel art, one picture per program (icons/*.png). Writes the
# .ico files the builds embed. Needs Pillow. Run from the repository root:
# python3 icons/make_icons.py
from PIL import Image

# Where each build takes its icon from.
dest = {
    "carnivores.png": "games/carnivores1/icon.ico",
    "carnivores-2.png": "games/carnivores2/icon.ico",
    "iceage.png": "games/carnivores-iceage/icon.ico",
    "omnivores-launcher.png": "launcher/icon.ico",
}
side = 256
for name, ico in dest.items():
    img = Image.open(f"icons/{name}").convert("RGBA")
    # Up to the largest icon without smudging the pixels: a whole number of
    # times over by copying them, then down the rest of the way by averaging,
    # which softens only their edges.
    n = -(-side // img.width)
    big = img.resize((img.width * n, img.height * n), Image.NEAREST)
    big = big.resize((side, side), Image.BOX)
    big.save(ico, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
