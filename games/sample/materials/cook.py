"""Compose the sample material recipe with the reusable offline cooker."""
import argparse
from pathlib import Path
import sys

SAMPLE_MATERIALS = Path(__file__).resolve().parent
sys.path.insert(0, str(SAMPLE_MATERIALS.parents[2] / 'tools'))
from bake_materials import bake


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    print(bake(SAMPLE_MATERIALS / 'recipe.py', args.output))
