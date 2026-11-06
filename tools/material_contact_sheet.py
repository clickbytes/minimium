"""Compose existing offline material previews for review (requires Pillow)."""
import argparse
from pathlib import Path
from PIL import Image, ImageDraw


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    roles = ('sand', 'soil', 'grass', 'litter', 'rock', 'snow')
    sheet = Image.new('RGB', (768, 6*284), (28, 30, 33))
    draw = ImageDraw.Draw(sheet)
    for row, role in enumerate(roles):
        albedo = Image.open(args.directory/f'{role}-albedo.png').convert('RGB')
        detail = Image.open(args.directory/f'{role}-detail.png').convert('RGB')
        repeat = Image.new('RGB', (1024, 1024))
        for y in (0, 512):
            for x in (0, 512):
                repeat.paste(albedo, (x, y))
        for col, (image, label) in enumerate(((albedo, 'whole tile'), (detail, 'lit source crop'), (repeat, '2 x 2 repeat'))):
            x, y = col*256, row*284
            draw.text((x+8, y+8), f'{role}: {label}', fill='white')
            sheet.paste(image.resize((256, 256), Image.Resampling.LANCZOS), (x, y+28))
    output = args.directory/'contact-sheet.png'
    sheet.save(output)
    print(output)


if __name__ == '__main__':
    main()
