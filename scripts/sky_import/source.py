"""Pinned HYG input validation and deterministic top-1000 selection."""

from __future__ import annotations

import csv
import hashlib
import heapq
import math
import re
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Iterator

SOURCE_COMMIT = "c7f7f883fe678cc7680169a50ccd7dcc49b060ce"
SOURCE_PATH = "hyg/CURRENT/hygdata_v41.csv"
SOURCE_SIZE = 33_932_548
SOURCE_GIT_BLOB_SHA1 = "ba2dec4eb0f6768914c7fc1051258100214ddf84"
SOURCE_SHA256 = "d9f69fd86bbf90a4e4d52b4c5c53eacfa6dfc0bfdef85bfd94f095e0bebe4ebd"
SOURCE_LICENSE_BLOB_SHA1 = "29773af75cb093d92b115bf22b6da18409c4cb62"
SOURCE_LICENSE_SHA256 = (
    "f404190403d31e0ce7223f4cb7af954485ad88077358330754dc5c892a856627"
)
SOURCE_LICENSE_SIZE = 422
SOURCE_LICENSE_NOTICE_PATH = "crates/assets/data/sky/HYG-LICENSE-NOTICE.txt"
MAX_SOURCE_BYTES = 64 * 1024 * 1024
MAX_SOURCE_ROWS = 200_000
MAX_RECORD_BYTES = 16 * 1024
MAX_STARS = 1000
U32_MAX = (1 << 32) - 1

_NUMBER = re.compile(r"[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?\Z")
_REQUIRED_COLUMNS = (
    "id",
    "hip",
    "hr",
    "proper",
    "ra",
    "dec",
    "mag",
    "ci",
    "x",
    "y",
    "z",
    "vx",
    "vy",
    "vz",
)


class ImportFailure(ValueError):
    """A source or record cannot be used to produce a deterministic asset."""


@dataclass(frozen=True)
class HYGStar:
    hyg_id: int
    hip_id: int
    hr_id: int
    proper_name: str
    magnitude: Decimal
    color_index: float
    source_position: tuple[str, str, str]
    position_pc: tuple[float, float, float]
    velocity_pc_per_jyear: tuple[float, float, float]
    missing_color: bool
    missing_proper_motion: bool


@dataclass(frozen=True)
class ImportReport:
    selected: tuple[HYGStar, ...]
    source_record_count: int
    eligible_record_count: int
    rejected_required_value_ids: tuple[int, ...]
    missing_proper_motion_ids: tuple[int, ...]
    missing_color_ids: tuple[int, ...]
    source_sha256: str


@dataclass(frozen=True)
class _WorstFirst:
    magnitude: Decimal
    hyg_id: int

    def __lt__(self, other: _WorstFirst) -> bool:
        return (self.magnitude, self.hyg_id) > (other.magnitude, other.hyg_id)


class _BoundedUtf8Lines(Iterator[str]):
    def __init__(self, path: Path) -> None:
        self._stream = path.open("rb")
        self.bytes_read = 0

    def __iter__(self) -> _BoundedUtf8Lines:
        return self

    def __next__(self) -> str:
        raw = self._stream.readline(MAX_RECORD_BYTES + 1)
        if not raw:
            raise StopIteration
        if len(raw) > MAX_RECORD_BYTES:
            raise ImportFailure("a physical CSV line exceeds 16 KiB")
        self.bytes_read += len(raw)
        try:
            return raw.decode("utf-8")
        except UnicodeDecodeError as error:
            raise ImportFailure("source CSV is not valid UTF-8") from error

    def close(self) -> None:
        self._stream.close()


def verify_source(path: Path) -> str:
    size = path.stat().st_size
    if size > MAX_SOURCE_BYTES:
        raise ImportFailure(f"source exceeds {MAX_SOURCE_BYTES} bytes")
    if size != SOURCE_SIZE:
        raise ImportFailure(f"source size is {size}, expected {SOURCE_SIZE}")
    git_hash = hashlib.sha1()
    git_hash.update(f"blob {size}\0".encode("ascii"))
    sha256 = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            git_hash.update(chunk)
            sha256.update(chunk)
    actual_git = git_hash.hexdigest()
    actual_sha = sha256.hexdigest()
    if actual_git != SOURCE_GIT_BLOB_SHA1:
        raise ImportFailure(
            f"source Git blob is {actual_git}, expected {SOURCE_GIT_BLOB_SHA1}"
        )
    if actual_sha != SOURCE_SHA256:
        raise ImportFailure(f"source SHA-256 is {actual_sha}, expected {SOURCE_SHA256}")
    return actual_sha


def verify_source_license_notice(content: bytes) -> None:
    if len(content) != SOURCE_LICENSE_SIZE:
        raise ImportFailure(
            f"HYG source license notice is {len(content)} bytes, expected {SOURCE_LICENSE_SIZE}"
        )
    git_hash = hashlib.sha1()
    git_hash.update(f"blob {len(content)}\0".encode("ascii"))
    git_hash.update(content)
    if git_hash.hexdigest() != SOURCE_LICENSE_BLOB_SHA1:
        raise ImportFailure("HYG source license notice Git blob does not match the pin")
    if hashlib.sha256(content).hexdigest() != SOURCE_LICENSE_SHA256:
        raise ImportFailure("HYG source license notice SHA-256 does not match the pin")


def load_top_stars(path: Path) -> ImportReport:
    source_sha256 = verify_source(path)
    lines = _BoundedUtf8Lines(path)
    reader = csv.reader(lines, strict=True)
    seen_ids: set[int] = set()
    rejected_ids: list[int] = []
    missing_pm: list[int] = []
    missing_color: list[int] = []
    heap: list[tuple[_WorstFirst, HYGStar]] = []
    eligible_count = 0
    row_count = 0
    try:
        before = lines.bytes_read
        header = next(reader, None)
        if header is None:
            raise ImportFailure("source CSV is empty")
        if lines.bytes_read - before > MAX_RECORD_BYTES:
            raise ImportFailure("CSV header exceeds 16 KiB")
        if len(header) != len(set(header)):
            raise ImportFailure("source CSV has duplicate column names")
        columns = {name: index for index, name in enumerate(header)}
        missing_columns = sorted(set(_REQUIRED_COLUMNS) - columns.keys())
        if missing_columns:
            raise ImportFailure(
                f"source CSV is missing columns: {', '.join(missing_columns)}"
            )

        while True:
            before = lines.bytes_read
            try:
                fields = next(reader)
            except StopIteration:
                break
            except csv.Error as error:
                raise ImportFailure(
                    f"CSV parse failed near record {row_count + 1}: {error}"
                ) from error
            row_count += 1
            if row_count > MAX_SOURCE_ROWS:
                raise ImportFailure(f"source exceeds {MAX_SOURCE_ROWS} data records")
            if lines.bytes_read - before > MAX_RECORD_BYTES:
                raise ImportFailure(f"CSV record {row_count} exceeds 16 KiB")
            if len(fields) != len(header):
                raise ImportFailure(
                    f"CSV record {row_count} has {len(fields)} fields, expected {len(header)}"
                )
            row = dict(zip(header, fields))
            hyg_id = _parse_id(row["id"], row_count)
            if hyg_id in seen_ids:
                raise ImportFailure(f"duplicate HYG id {hyg_id} at record {row_count}")
            seen_ids.add(hyg_id)
            if hyg_id == 0:
                continue

            star, rejected = _parse_star(row, hyg_id)
            if rejected:
                rejected_ids.append(hyg_id)
                continue
            assert star is not None
            eligible_count += 1
            key = _WorstFirst(star.magnitude, star.hyg_id)
            if len(heap) < MAX_STARS:
                heapq.heappush(heap, (key, star))
            elif heap[0][0] < key:
                heapq.heapreplace(heap, (key, star))

        if eligible_count < MAX_STARS:
            raise ImportFailure(
                f"only {eligible_count} eligible stars; exactly {MAX_STARS} are required"
            )
        selected = tuple(
            sorted(
                (entry[1] for entry in heap),
                key=lambda item: (item.magnitude, item.hyg_id),
            )
        )
        missing_pm = [
            star.hyg_id
            for star in (entry[1] for entry in heap)
            if star.missing_proper_motion
        ]
        missing_color = [
            star.hyg_id for star in (entry[1] for entry in heap) if star.missing_color
        ]
        return ImportReport(
            selected=selected,
            source_record_count=row_count,
            eligible_record_count=eligible_count,
            rejected_required_value_ids=tuple(sorted(rejected_ids)),
            missing_proper_motion_ids=tuple(sorted(missing_pm)),
            missing_color_ids=tuple(sorted(missing_color)),
            source_sha256=source_sha256,
        )
    finally:
        lines.close()


def _parse_id(value: str, record: int) -> int:
    text = value.strip()
    if not text or len(text) > 10 or not text.isascii() or not text.isdecimal():
        raise ImportFailure(f"record {record} has an invalid HYG id")
    result = int(text)
    if result > U32_MAX:
        raise ImportFailure(f"record {record} HYG id does not fit u32")
    return result


def _parse_star(row: dict[str, str], hyg_id: int) -> tuple[HYGStar | None, bool]:
    hip_id = _parse_optional_id(row["hip"], "Hipparcos", hyg_id)
    hr_id = _parse_optional_id(row["hr"], "HR", hyg_id)
    color_text = row["ci"].strip()
    missing_color = not color_text
    color_index = 0.65 if missing_color else _float_value(color_text, "ci", hyg_id)
    velocity_text = [row[key].strip() for key in ("vx", "vy", "vz")]
    missing_velocity = [not value for value in velocity_text]
    if all(missing_velocity):
        velocity_values = (0.0, 0.0, 0.0)
        missing_proper_motion = True
    elif any(missing_velocity):
        raise ImportFailure(f"HYG id {hyg_id} has partially missing velocity")
    else:
        velocity_values = tuple(
            _float_value(value, name, hyg_id)
            for name, value in zip(("vx", "vy", "vz"), velocity_text)
        )
        missing_proper_motion = False

    magnitude = _decimal_or_missing(row["mag"], "mag", hyg_id)
    ra = _decimal_or_missing(row["ra"], "ra", hyg_id)
    dec = _decimal_or_missing(row["dec"], "dec", hyg_id)
    position = tuple(
        _float_or_missing(row[key], key, hyg_id) for key in ("x", "y", "z")
    )
    if (
        magnitude is None
        or ra is None
        or dec is None
        or any(value is None for value in position)
    ):
        return None, True
    if not (Decimal(0) <= ra < Decimal(24) and Decimal(-90) <= dec <= Decimal(90)):
        return None, True
    position_values = tuple(float(value) for value in position if value is not None)
    if (
        math.hypot(
            math.hypot(position_values[0], position_values[1]), position_values[2]
        )
        == 0.0
    ):
        return None, True

    star = HYGStar(
        hyg_id=hyg_id,
        hip_id=hip_id,
        hr_id=hr_id,
        proper_name=row["proper"].strip(),
        magnitude=magnitude,
        color_index=color_index,
        source_position=tuple(row[key].strip() for key in ("x", "y", "z")),
        position_pc=position_values,
        velocity_pc_per_jyear=velocity_values,
        missing_color=missing_color,
        missing_proper_motion=missing_proper_motion,
    )
    return star, False


def _parse_optional_id(value: str, field: str, hyg_id: int) -> int:
    text = value.strip()
    if not text:
        return 0
    if len(text) > 10 or not text.isascii() or not text.isdecimal():
        raise ImportFailure(f"HYG id {hyg_id} has an invalid {field} id")
    result = int(text)
    if result > U32_MAX:
        raise ImportFailure(f"HYG id {hyg_id} {field} id does not fit u32")
    return result


def _decimal_or_missing(value: str, field: str, hyg_id: int) -> Decimal | None:
    text = value.strip()
    return None if not text else _decimal_value(text, field, hyg_id)


def _decimal_value(text: str, field: str, hyg_id: int) -> Decimal:
    if not _NUMBER.fullmatch(text):
        raise ImportFailure(f"HYG id {hyg_id} has invalid {field} value {text!r}")
    try:
        value = Decimal(text)
    except InvalidOperation as error:
        raise ImportFailure(f"HYG id {hyg_id} has invalid {field} value") from error
    if not value.is_finite():
        raise ImportFailure(f"HYG id {hyg_id} has non-finite {field}")
    return value


def _float_or_missing(value: str, field: str, hyg_id: int) -> float | None:
    text = value.strip()
    return None if not text else _float_value(text, field, hyg_id)


def _float_value(text: str, field: str, hyg_id: int) -> float:
    value = float(_decimal_value(text, field, hyg_id))
    if not math.isfinite(value):
        raise ImportFailure(f"HYG id {hyg_id} {field} overflows f64")
    return value
