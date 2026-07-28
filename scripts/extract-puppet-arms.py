"""Extract the generated 4x2 puppet-arm sheet into trimmed PNG layers."""

from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tmp" / "imagegen" / "puppet-arms-alpha.png"
OUTPUT = ROOT / "assets" / "puppet"

NAMES = (
    "left-upper.png",
    "left-forearm.png",
    "left-hand.png",
    "left-joint.png",
    "right-upper.png",
    "right-forearm.png",
    "right-hand.png",
    "right-joint.png",
)


def trim(image: Image.Image, padding: int = 10) -> Image.Image:
    alpha = image.getchannel("A")
    bounds = alpha.getbbox()
    if bounds is None:
        raise RuntimeError("component has no visible pixels")
    left, top, right, bottom = bounds
    left = max(0, left - padding)
    top = max(0, top - padding)
    right = min(image.width, right + padding)
    bottom = min(image.height, bottom + padding)
    return image.crop((left, top, right, bottom))


def main() -> None:
    source = Image.open(SOURCE).convert("RGBA")
    OUTPUT.mkdir(parents=True, exist_ok=True)
    cell_width = source.width // 4
    cell_height = source.height // 2

    for index, name in enumerate(NAMES):
        row, column = divmod(index, 4)
        component = source.crop(
            (
                column * cell_width,
                row * cell_height,
                (column + 1) * cell_width,
                (row + 1) * cell_height,
            )
        )
        trim(component).save(OUTPUT / name, optimize=True)


if __name__ == "__main__":
    main()
