"""Stable little-endian asset, provenance files, and transactional publishing."""

from __future__ import annotations

import hashlib
import json
import math
import os
import shutil
import struct
import tempfile
from pathlib import Path

from .source import (
    MAX_STARS,
    SOURCE_COMMIT,
    SOURCE_GIT_BLOB_SHA1,
    SOURCE_LICENSE_BLOB_SHA1,
    SOURCE_LICENSE_SHA256,
    SOURCE_LICENSE_SIZE,
    SOURCE_LICENSE_NOTICE_PATH,
    SOURCE_PATH,
    SOURCE_SIZE,
    HYGStar,
    ImportFailure,
    ImportReport,
)

SCHEMA_VERSION = 1
CATALOGUE_ID = 2
MAGIC = b"MNSKY1K0"
LABELS_PATH = "crates/assets/data/sky/star-labels-v1.tsv"
HEADER_BYTES = 96
RECORD_BYTES = 64
EPOCH_JYEAR = 2000.0
LICENSE_NAME = "crates/assets/data/sky/LICENSE-CC-BY-SA-4.0.txt"
ATTRIBUTION = """# HYG 4.1 star catalogue attribution

Source: HYG Database v4.1 by Astronexus / David Nash, archived at
<https://github.com/astronexus/HYG-Database>.

Pinned input: `hyg/CURRENT/hygdata_v41.csv` at commit
`c7f7f883fe678cc7680169a50ccd7dcc49b060ce`. This asset selects the 1000
valid records with the lowest apparent visual magnitude, breaking exact ties
by HYG id. It excludes the Sun, removes unused columns, and converts the
selected records to the documented little-endian `stars-v1.bin` format.
Missing color index values use the documented neutral value 0.65; missing
proper-motion vectors are zero-filled. Numeric source coordinates are retained.
The aligned label asset uses HYG proper names where printable in the bundled
font, with HIP/HR/HYG identifiers as fallbacks.

The HYG data and this derived catalogue are licensed under
[Creative Commons Attribution-ShareAlike 4.0 International](https://creativecommons.org/licenses/by-sa/4.0/).
See `LICENSE-CC-BY-SA-4.0.txt` for the full license text and
`HYG-LICENSE-NOTICE.txt` for the exact upstream notice. Source license notice:
<https://github.com/astronexus/HYG-Database/blob/c7f7f883fe678cc7680169a50ccd7dcc49b060ce/LICENSE>.
"""


def build_outputs(
    report: ImportReport, license_text: bytes, source_license_notice: bytes
) -> dict[str, bytes]:
    license_digest = hashlib.sha256(license_text).hexdigest()
    if not license_text.startswith(b"Attribution-ShareAlike 4.0 International"):
        raise ImportFailure("CC BY-SA 4.0 legal text is missing or unrecognized")
    source_license_digest = hashlib.sha256(source_license_notice).hexdigest()
    source_license_git_hash = hashlib.sha1(
        f"blob {len(source_license_notice)}\0".encode("ascii") + source_license_notice
    ).hexdigest()
    if (
        len(source_license_notice) != SOURCE_LICENSE_SIZE
        or source_license_digest != SOURCE_LICENSE_SHA256
        or source_license_git_hash != SOURCE_LICENSE_BLOB_SHA1
    ):
        raise ImportFailure(
            "HYG source license notice does not match its pinned digest"
        )
    payload = b"".join(_pack_record(star) for star in report.selected)
    labels = "".join(_label_line(star) for star in report.selected).encode("utf-8")
    payload_sha = hashlib.sha256(payload).digest()
    source_sha = bytes.fromhex(report.source_sha256)
    header = struct.pack(
        "<8sHHIIId32s32s",
        MAGIC,
        SCHEMA_VERSION,
        RECORD_BYTES,
        CATALOGUE_ID,
        len(report.selected),
        0,
        EPOCH_JYEAR,
        source_sha,
        payload_sha,
    )
    asset = header + payload
    asset_sha = hashlib.sha256(asset).digest()
    manifest = _manifest(report, asset, payload, labels, license_digest)
    identity = _identity_source(asset_sha, source_sha)
    _validate_products(asset, manifest, labels, identity, license_text, source_license_notice)
    return {
        "crates/assets/data/sky/stars-v1.bin": asset,
        "crates/assets/data/sky/stars-v1.manifest.json": manifest,
        LABELS_PATH: labels,
        LICENSE_NAME: license_text,
        SOURCE_LICENSE_NOTICE_PATH: source_license_notice,
        "crates/assets/data/sky/ATTRIBUTION.md": ATTRIBUTION.encode("utf-8"),
        "crates/assets/src/sky_catalog/identity.generated.rs": identity,
    }


def publish_outputs(root: Path, outputs: dict[str, bytes]) -> None:
    if not root.is_dir():
        raise ImportFailure(f"output root does not exist: {root}")
    stage = Path(tempfile.mkdtemp(prefix=".hyg-stars-", dir=root))
    staged: dict[str, Path] = {}
    backups: dict[str, Path] = {}
    published: list[str] = []
    try:
        for relative, content in outputs.items():
            path = stage / "new" / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            staged[relative] = path
        _validate_staged(staged)

        for relative in outputs:
            destination = root / relative
            if destination.exists() and not destination.is_file():
                raise ImportFailure(f"output path is not a regular file: {relative}")
            destination.parent.mkdir(parents=True, exist_ok=True)
            backup = stage / "backup" / relative
            if destination.exists():
                backup.parent.mkdir(parents=True, exist_ok=True)
                os.replace(destination, backup)
                backups[relative] = backup
            os.replace(staged[relative], destination)
            published.append(relative)
    except Exception:
        for relative in reversed(list(outputs)):
            destination = root / relative
            backup = backups.get(relative)
            if relative in published and destination.exists():
                destination.unlink()
            if backup is not None and backup.exists():
                destination.parent.mkdir(parents=True, exist_ok=True)
                os.replace(backup, destination)
        raise
    finally:
        shutil.rmtree(stage, ignore_errors=True)


def _pack_record(star: HYGStar) -> bytes:
    magnitude = _finite_f32(float(star.magnitude), "apparent magnitude", star.hyg_id)
    color_index = _finite_f32(star.color_index, "color index", star.hyg_id)
    values = (
        *star.position_pc,
        *star.velocity_pc_per_jyear,
    )
    if not all(math.isfinite(value) for value in values):
        raise ImportFailure(f"HYG id {star.hyg_id} has non-finite astrometry")
    if math.hypot(math.hypot(values[0], values[1]), values[2]) == 0.0:
        raise ImportFailure(f"HYG id {star.hyg_id} has a zero position vector")
    try:
        record = struct.pack(
            "<IIff6d",
            star.hyg_id,
            star.hip_id,
            magnitude,
            color_index,
            *values,
        )
    except (OverflowError, struct.error) as error:
        raise ImportFailure(
            f"HYG id {star.hyg_id} cannot be encoded in stars-v1"
        ) from error
    if len(record) != RECORD_BYTES:
        raise ImportFailure("internal record layout is not 64 bytes")
    return record


def _finite_f32(value: float, field: str, hyg_id: int) -> float:
    if not math.isfinite(value):
        raise ImportFailure(f"HYG id {hyg_id} {field} is not finite")
    try:
        packed = struct.pack("<f", value)
    except (OverflowError, struct.error) as error:
        raise ImportFailure(f"HYG id {hyg_id} {field} does not fit f32") from error
    converted = struct.unpack("<f", packed)[0]
    if not math.isfinite(converted):
        raise ImportFailure(f"HYG id {hyg_id} {field} overflows f32")
    return converted


def _manifest(
    report: ImportReport, asset: bytes, payload: bytes, labels: bytes, license_digest: str
) -> bytes:
    selected = report.selected
    repeated_hip = _repeated_catalogue_ids(selected, "hip_id")
    repeated_hr = _repeated_catalogue_ids(selected, "hr_id")
    repeated_positions = _repeated_positions(selected)
    asset_sha = hashlib.sha256(asset).hexdigest()
    payload_sha = hashlib.sha256(payload).hexdigest()
    value = {
        "asset_sha256": asset_sha,
        "catalogue_id": CATALOGUE_ID,
        "epoch_jyear": EPOCH_JYEAR,
        "format": {
            "header_bytes": HEADER_BYTES,
            "magic": MAGIC.decode("ascii"),
            "record_bytes": RECORD_BYTES,
            "record_count": len(selected),
            "schema_version": SCHEMA_VERSION,
            "total_bytes": len(asset),
        },
        "importer_version": 1,
        "input_audit": {
            "eligible_record_count": report.eligible_record_count,
            "rejected_required_value_count": len(report.rejected_required_value_ids),
            "rejected_required_value_ids": list(report.rejected_required_value_ids),
            "source_record_count": report.source_record_count,
            "repeated_nonempty_hip_ids": repeated_hip,
            "repeated_nonempty_hr_ids": repeated_hr,
            "identical_source_position_groups": repeated_positions,
        },
        "license": {
            "catalogue_license": "CC BY-SA 4.0",
            "full_text_path": LICENSE_NAME,
            "full_text_sha256": license_digest,
            "source_license_notice_path": SOURCE_LICENSE_NOTICE_PATH,
            "source_license_git_blob_sha1": SOURCE_LICENSE_BLOB_SHA1,
            "source_license_sha256": SOURCE_LICENSE_SHA256,
            "source_license_size": SOURCE_LICENSE_SIZE,
            "source_license_url": f"https://github.com/astronexus/HYG-Database/blob/{SOURCE_COMMIT}/LICENSE",
            "url": "https://creativecommons.org/licenses/by-sa/4.0/",
        },
        "payload_sha256": payload_sha,
        "labels": {"path": LABELS_PATH, "sha256": hashlib.sha256(labels).hexdigest(), "count": len(selected)},
        "selection": {
            "policy_version": 1,
            "rule": "lowest Decimal(apparent visual magnitude), then ascending HYG id; exclude id 0",
            "selected_count": len(selected),
            "selected_ids": [star.hyg_id for star in selected],
            "threshold_magnitude_decimal": str(selected[-1].magnitude),
        },
        "source": {
            "commit": SOURCE_COMMIT,
            "git_blob_sha1": SOURCE_GIT_BLOB_SHA1,
            "path": SOURCE_PATH,
            "repository": "https://github.com/astronexus/HYG-Database",
            "sha256": report.source_sha256,
            "size_bytes": SOURCE_SIZE,
            "url": f"https://raw.githubusercontent.com/astronexus/HYG-Database/{SOURCE_COMMIT}/{SOURCE_PATH}",
        },
        "source_csv_sha256": report.source_sha256,
        "missing_color_ids": list(report.missing_color_ids),
        "missing_proper_motion_ids": list(report.missing_proper_motion_ids),
    }
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def _identity_source(asset_sha: bytes, source_sha: bytes) -> bytes:
    def rust_array(raw: bytes) -> str:
        return ", ".join(f"0x{byte:02x}" for byte in raw)

    text = (
        "// Generated by scripts/import_hyg_stars.py v1; do not edit by hand.\n"
        f"pub(super) const CATALOGUE_ID: u32 = {CATALOGUE_ID};\n"
        f"pub(super) const ASSET_SHA256: [u8; 32] = [{rust_array(asset_sha)}];\n"
        f"pub(super) const SOURCE_CSV_SHA256: [u8; 32] = [{rust_array(source_sha)}];\n"
    )
    return text.encode("utf-8")


def _label_line(star: HYGStar) -> str:
    proper = star.proper_name
    if proper and proper.isascii() and proper.isprintable() and len(proper) <= 30:
        label = proper
    elif star.hip_id:
        label = f"HIP {star.hip_id}"
    elif star.hr_id:
        label = f"HR {star.hr_id}"
    else:
        label = f"HYG {star.hyg_id}"
    return f"{star.hyg_id}\t{label}\n"


def _repeated_catalogue_ids(selected: tuple[HYGStar, ...], field: str) -> list[int]:
    counts: dict[int, int] = {}
    for star in selected:
        value = getattr(star, field)
        if value:
            counts[value] = counts.get(value, 0) + 1
    return sorted(value for value, count in counts.items() if count > 1)


def _repeated_positions(selected: tuple[HYGStar, ...]) -> list[list[int]]:
    groups: dict[tuple[str, str, str], list[int]] = {}
    for star in selected:
        groups.setdefault(star.source_position, []).append(star.hyg_id)
    return sorted(
        (sorted(ids) for ids in groups.values() if len(ids) > 1), key=lambda ids: ids[0]
    )


def _validate_products(
    asset: bytes,
    manifest: bytes,
    labels: bytes,
    identity: bytes,
    license_text: bytes,
    source_license_notice: bytes,
) -> None:
    if len(asset) != HEADER_BYTES + MAX_STARS * RECORD_BYTES:
        raise ImportFailure("generated asset has an invalid total length")
    header = struct.unpack("<8sHHIIId32s32s", asset[:HEADER_BYTES])
    if header[:6] != (
        MAGIC,
        SCHEMA_VERSION,
        RECORD_BYTES,
        CATALOGUE_ID,
        MAX_STARS,
        0,
    ):
        raise ImportFailure("generated binary header does not match the V1 contract")
    if header[6] != EPOCH_JYEAR:
        raise ImportFailure("generated binary epoch does not match the V1 contract")
    try:
        parsed_manifest = json.loads(manifest)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ImportFailure(
            "generated manifest is not valid deterministic JSON"
        ) from error
    if parsed_manifest.get("asset_sha256") != hashlib.sha256(asset).hexdigest():
        raise ImportFailure("manifest asset digest does not match generated binary")
    if (
        parsed_manifest.get("payload_sha256")
        != hashlib.sha256(asset[HEADER_BYTES:]).hexdigest()
    ):
        raise ImportFailure("manifest payload digest does not match generated binary")
    if hashlib.sha256(asset[HEADER_BYTES:]).digest() != asset[64:96]:
        raise ImportFailure("binary payload digest does not match its header")
    if parsed_manifest.get("source_csv_sha256") != header[7].hex():
        raise ImportFailure("manifest source digest does not match binary header")
    label_lines = labels.decode("utf-8").splitlines()
    selected_ids = parsed_manifest.get("selection", {}).get("selected_ids", [])
    if len(label_lines) != MAX_STARS or [line.split("\t", 1)[0] for line in label_lines] != [str(value) for value in selected_ids]:
        raise ImportFailure("labels do not align with selected star IDs")
    if any("\t" not in line or not line.split("\t", 1)[1] for line in label_lines):
        raise ImportFailure("a generated star label is empty")
    if parsed_manifest.get("labels") != {"path": LABELS_PATH, "sha256": hashlib.sha256(labels).hexdigest(), "count": MAX_STARS}:
        raise ImportFailure("manifest labels digest or count does not match")
    if (
        parsed_manifest.get("license", {}).get("full_text_sha256")
        != hashlib.sha256(license_text).hexdigest()
    ):
        raise ImportFailure(
            "manifest license digest does not match the included legal text"
        )
    if (
        parsed_manifest.get("license", {}).get("source_license_sha256")
        != hashlib.sha256(source_license_notice).hexdigest()
    ):
        raise ImportFailure("manifest does not match the included HYG license notice")
    notice_git_hash = hashlib.sha1(
        f"blob {len(source_license_notice)}\0".encode("ascii") + source_license_notice
    ).hexdigest()
    if parsed_manifest.get("license", {}).get(
        "source_license_git_blob_sha1"
    ) != notice_git_hash or parsed_manifest.get("license", {}).get(
        "source_license_size"
    ) != len(source_license_notice):
        raise ImportFailure("manifest does not match HYG source license blob identity")
    selected = parsed_manifest.get("selection", {})
    if (
        selected.get("selected_count") != MAX_STARS
        or len(selected.get("selected_ids", [])) != MAX_STARS
    ):
        raise ImportFailure(f"manifest does not describe exactly {MAX_STARS} selected IDs")
    expected_identity = _identity_source(hashlib.sha256(asset).digest(), asset[32:64])
    if identity != expected_identity:
        raise ImportFailure(
            "generated Rust identity does not match the generated binary"
        )


def _validate_staged(staged: dict[str, Path]) -> None:
    asset = staged["crates/assets/data/sky/stars-v1.bin"].read_bytes()
    manifest = staged["crates/assets/data/sky/stars-v1.manifest.json"].read_bytes()
    labels = staged[LABELS_PATH].read_bytes()
    identity = staged[
        "crates/assets/src/sky_catalog/identity.generated.rs"
    ].read_bytes()
    license_text = staged[LICENSE_NAME].read_bytes()
    source_license_notice = staged[SOURCE_LICENSE_NOTICE_PATH].read_bytes()
    _validate_products(asset, manifest, labels, identity, license_text, source_license_notice)
