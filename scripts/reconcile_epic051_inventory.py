"""Attach integrated E051 provenance to the frozen declaration inventory."""

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/plan/epic-051-inventory.json"
ISSUE_COMMITS = {
    570: "2993820", 573: "2bf9799", 574: "32cc544", 575: "89d29fa",
    576: "1f8c743", 579: "5d0f721", 580: "ea14a79", 581: "5986479",
    582: "8d56ba6", 583: "be8a9d6", 584: "2aa6a4e", 585: "c84e1a6",
    586: "f4ba411", 587: "9ffc52b", 588: "34dbb8b", 589: "43b86a5",
    590: "073aa34", 591: "7103de7", 592: "e5e7c7a", 593: "6245fc9",
}


def registration_path(row, files):
    name = row["canonical"]
    quoted = '"' + name + '"'
    candidates = [p for p, content in files.items() if quoted in content]
    if not candidates:
        raise ValueError(f"{row['id']}: no source registration for {name}")
    catalog = row["catalog"]

    def rank(path):
        s = path.as_posix()
        if catalog in ("client", "pre-parser presentation") or row["id"].startswith("A"):
            return (0 if "apps/client/src/client_commands/catalog" in s else 1 if "apps/client/src/client_commands" in s else 2, s)
        if catalog == "proposed" and name.startswith("client."):
            return (0 if "apps/client/src/client_commands/catalog" in s else 1, s)
        return (0 if "crates/server-runtime/src/server_command" in s else 1 if "crates/server-runtime/src/commands" in s else 2, s)

    return min(candidates, key=rank).relative_to(ROOT).as_posix()


def main():
    data = json.loads(INVENTORY.read_text(encoding="utf-8"))
    rows = data["records"]
    if len(rows) != 147 or len({row["id"] for row in rows}) != 147:
        raise ValueError("expected 142 frozen declarations plus five added operations")
    source_roots = (ROOT / "apps/client/src", ROOT / "apps/server/src", ROOT / "crates")
    files = {
        path: path.read_text(encoding="utf-8", errors="replace")
        for base in source_roots
        for path in base.rglob("*.rs")
    }
    commits = {
        issue: subprocess.check_output(
            ["git", "rev-parse", sha], cwd=ROOT, text=True
        ).strip()
        for issue, sha in ISSUE_COMMITS.items()
    }
    for row in rows:
        issue = row["migration_issue"]
        registration = registration_path(row, files)
        content = files[ROOT / registration]
        line = next(
            number for number, text in enumerate(content.splitlines(), 1)
            if '"' + row["canonical"] + '"' in text
        )
        row["current"] = {
            "status": "integrated",
            "issue": issue,
            "commit": commits[issue],
            "registration": registration,
            "line": line,
        }
    INVENTORY.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Reconciled {len(rows)} inventory rows against current Rust registrations.")


if __name__ == "__main__":
    main()
