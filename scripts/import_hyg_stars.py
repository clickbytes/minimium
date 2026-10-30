#!/usr/bin/env python3
"""Build the pinned HYG 4.1 star asset from a local CSV, without network access."""

from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

from sky_import.binary import LICENSE_NAME, build_outputs, publish_outputs
from sky_import.source import (
    SOURCE_LICENSE_NOTICE_PATH,
    ImportFailure,
    load_top_stars,
    verify_source_license_notice,
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--input", required=True, type=Path, help="local pinned hygdata_v41.csv"
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="repository root for generated assets",
    )
    args = parser.parse_args()

    repository_root = Path(__file__).resolve().parents[1]
    license_path = repository_root / LICENSE_NAME
    source_license_path = repository_root / SOURCE_LICENSE_NOTICE_PATH
    try:
        legal_text = license_path.read_bytes()
        source_license_notice = source_license_path.read_bytes()
        verify_source_license_notice(source_license_notice)
        report = load_top_stars(args.input)
        outputs = build_outputs(report, legal_text, source_license_notice)
        publish_outputs(args.output.resolve(), outputs)
    except (ImportFailure, OSError) as error:
        print(f"HYG import failed: {error}", file=sys.stderr)
        return 1

    asset = outputs["crates/assets/data/sky/stars-v1.bin"]
    print(f"source_sha256={report.source_sha256}")
    print(f"source_records={report.source_record_count}")
    print(f"eligible_records={report.eligible_record_count}")
    print(f"selected_records={len(report.selected)}")
    print(f"threshold_magnitude={report.selected[-1].magnitude}")
    print(f"asset_bytes={len(asset)}")
    print(f"asset_sha256={hashlib.sha256(asset).hexdigest()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
