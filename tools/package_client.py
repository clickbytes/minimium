"""Package a client with an explicitly selected, validated material manifest."""
import argparse
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--manifest', required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    subprocess.run([
        'cargo', 'run', '--locked', '--manifest-path', str(root/'Cargo.toml'),
        '-p', 'minimium-asset-io', '--example', 'package_client', '--',
        str(args.executable.resolve()), str(args.manifest.resolve()),
        str(args.output.resolve()),
    ], check=True)


if __name__ == '__main__':
    main()
