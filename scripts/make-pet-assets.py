"""Build the shorter, layered 椰椰 sprite from the approved transparent source."""

from pathlib import Path
from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "assets" / "pet.png"
OUT_DIR = ROOT / "assets"
BREAK_Y = 610
LOWER_SCALE = 0.64
PADDING = 22


def arm_mask(size, points):
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).polygon(points, fill=255)
    return mask


def compress_lower(image):
    upper = image.crop((0, 0, image.width, BREAK_Y))
    lower = image.crop((0, BREAK_Y, image.width, image.height))
    lower = lower.resize(
        (lower.width, round(lower.height * LOWER_SCALE)),
        Image.Resampling.LANCZOS,
    )
    result = Image.new("RGBA", (image.width, upper.height + lower.height), (0, 0, 0, 0))
    result.alpha_composite(upper)
    result.alpha_composite(lower, (0, upper.height))
    return result


source = Image.open(SOURCE).convert("RGBA")
left_mask = arm_mask(
    source.size,
    [(195, 385), (208, 354), (250, 350), (465, 555), (466, 610),
     (428, 670), (375, 650), (200, 470)],
)
right_mask = arm_mask(
    source.size,
    [(790, 565), (1002, 402), (1045, 408), (1062, 444), (1051, 510),
     (885, 678), (832, 684), (800, 635)],
)

left_arm = Image.new("RGBA", source.size, (0, 0, 0, 0))
right_arm = Image.new("RGBA", source.size, (0, 0, 0, 0))
left_arm.paste(source, mask=left_mask)
right_arm.paste(source, mask=right_mask)

body = source.copy()
body_alpha = body.getchannel("A")
# Remove only the distal part of each original arm. The retained upper-arm
# material acts as a soft shoulder socket beneath the animated full-arm layer.
left_erase = arm_mask(
    source.size,
    [(195, 385), (208, 354), (250, 350), (412, 505), (382, 584), (200, 470)],
)
right_erase = arm_mask(
    source.size,
    [(850, 585), (1002, 402), (1045, 408), (1062, 444), (1051, 510), (884, 650)],
)
erase = ImageChops.lighter(left_erase, right_erase).filter(ImageFilter.MinFilter(81))
body.putalpha(ImageChops.subtract(body_alpha, erase))

layers = [compress_lower(layer) for layer in (body, left_arm, right_arm)]
combined_alpha = Image.new("L", layers[0].size, 0)
for layer in layers:
    combined_alpha = Image.composite(
        Image.new("L", layer.size, 255), combined_alpha, layer.getchannel("A")
    )
bbox = combined_alpha.getbbox()
if not bbox:
    raise RuntimeError("Sprite extraction produced an empty image")
bbox = (
    max(0, bbox[0] - PADDING),
    max(0, bbox[1] - PADDING),
    min(layers[0].width, bbox[2] + PADDING),
    min(layers[0].height, bbox[3] + PADDING),
)

names = ("pet-puppet-body.png", "pet-cute-left-arm.png", "pet-cute-right-arm.png")
for layer, name in zip(layers, names):
    layer.crop(bbox).save(OUT_DIR / name)

preview = Image.new("RGBA", (bbox[2] - bbox[0], bbox[3] - bbox[1]), (0, 0, 0, 0))
for layer in layers:
    preview.alpha_composite(layer.crop(bbox))
preview.save(OUT_DIR / "pet-cute-preview.png")
print(f"Wrote layered sprite {preview.size[0]}x{preview.size[1]}")
