"""Copy a compiled client and its cooked offline materials into a distributable folder."""
import argparse
from pathlib import Path
import shutil


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    materials = root / 'target/materials/sample-v1'
    if not args.executable.is_file() or not (materials/'manifest.ron').is_file():
        parser.error('build the client first (Python + numpy required by the material cooker)')
    args.output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(args.executable, args.output/args.executable.name)
    shutil.copytree(materials, args.output/'materials/sample-v1', dirs_exist_ok=True)


if __name__ == '__main__':
    main()
