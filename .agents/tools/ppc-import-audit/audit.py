#!/usr/bin/env python3
"""Rank PowerPC imports that Systemless cannot satisfy across many archives.

Runs `systemless --ppc-import-audit` over every catalogue entry with a PowerPC
architecture (and any extra archives given on the command line), then writes a
Markdown summary ranking each missing symbol by the number of titles that
import it. Catalogue archives are fetched by SHA-256 from the public asset host
into a cache directory outside the repository.

    python3 -I .agents/tools/ppc-import-audit/audit.py \
        --binary target/ci-test/systemless --out /tmp/ppc-audit [EXTRA_ARCHIVE...]
"""

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import urllib.request

ASSET_HOST = "https://assets.systemless.org/catalogue/objects/sha256"
REPO_ROOT = pathlib.Path(__file__).resolve().parents[3]


def catalogue_ppc_archives(catalogue_dir):
    """Yield (entry id, sha256, format) for each PowerPC entry's archive."""
    for path in sorted(catalogue_dir.glob("*.md")):
        text = path.read_text(encoding="utf-8")
        match = re.match(r"---\n(.*?)\n---\n", text, re.S)
        if not match:
            continue
        front = match.group(1)
        arches = re.search(r"^architectures:\n((?:- .*\n)+)", front + "\n", re.M)
        if not arches or "- ppc" not in arches.group(1):
            continue
        for block in re.split(r"^- id: ", front, flags=re.M)[1:]:
            if not re.search(r"^  role: archive$", block, re.M):
                continue
            sha = re.search(r"^    sha256: ([0-9a-f]{64})$", block, re.M)
            fmt = re.search(r"^  format: (\w+)$", block, re.M)
            if sha and fmt:
                yield path.stem, sha.group(1), fmt.group(1)


def fetch(sha, fmt, cache_dir):
    target = cache_dir / f"{sha}.{fmt}"
    if target.exists():
        return target
    url = f"{ASSET_HOST}/{sha[:2]}/{sha}.{fmt}"
    print(f"fetch {url}", file=sys.stderr)
    partial = target.with_suffix(target.suffix + ".part")
    digest = hashlib.sha256()
    # The asset host rejects urllib's default user agent.
    request = urllib.request.Request(url, headers={"User-Agent": "systemless-ppc-import-audit"})
    with urllib.request.urlopen(request) as response, open(partial, "wb") as out:
        while chunk := response.read(1 << 20):
            digest.update(chunk)
            out.write(chunk)
    if digest.hexdigest() != sha:
        partial.unlink()
        raise RuntimeError(f"checksum mismatch for {url}")
    partial.rename(target)
    return target


def audit(binary, archive):
    result = subprocess.run(
        [str(binary), str(archive), "--ppc-import-audit"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip()[-2000:])
    return json.loads(result.stdout)


def summarise(reports):
    titles_by_gap = {}
    lines = ["# PowerPC import gaps", ""]
    lines.append("| Title | Fragments | Unsupported | Weak unresolved | Load error |")
    lines.append("|---|---:|---:|---:|---|")
    for title, report in sorted(reports.items()):
        gaps = {}
        for fragment in report["fragments"]:
            name = fragment["name"] or fragment["path"].rsplit("/", 1)[-1]
            for entry in fragment["imports"]:
                if entry["status"] in ("unsupported", "weak_unresolved"):
                    key = (entry["status"], entry["library"], entry["symbol"])
                    gaps.setdefault(key, []).append(name)
        for key, fragments in gaps.items():
            titles_by_gap.setdefault(key, []).append(f"{title} ({', '.join(sorted(set(fragments)))})")
        unsupported = sum(1 for key in gaps if key[0] == "unsupported")
        weak = len(gaps) - unsupported
        error = (report.get("load_error") or "").replace("|", "\\|")[:80]
        lines.append(
            f"| {title} | {len(report['fragments'])} | {unsupported} | {weak} | {error} |"
        )
    for status, heading in (
        ("unsupported", "Unsupported strong imports (halt when first called)"),
        ("weak_unresolved", "Unresolved weak imports (bound to address 0)"),
    ):
        ranked = sorted(
            ((titles, key) for key, titles in titles_by_gap.items() if key[0] == status),
            key=lambda item: (-len(item[0]), item[1][1], item[1][2]),
        )
        lines += ["", f"## {heading}", "", "| Titles | Library | Symbol | Imported by (fragment) |"]
        lines.append("|---:|---|---|---|")
        for titles, (_, library, symbol) in ranked:
            lines.append(f"| {len(titles)} | {library} | `{symbol}` | {', '.join(titles)} |")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", type=pathlib.Path, default=REPO_ROOT / "target/ci-test/systemless")
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--cache", type=pathlib.Path, default=pathlib.Path.home() / ".cache/systemless-ppc-import-audit")
    parser.add_argument("--catalogue", type=pathlib.Path, default=REPO_ROOT / "www/catalogue")
    parser.add_argument("--no-catalogue", action="store_true", help="audit only the extra archives")
    parser.add_argument("archives", nargs="*", type=pathlib.Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    args.cache.mkdir(parents=True, exist_ok=True)

    targets = [(path.name, path) for path in args.archives]
    if not args.no_catalogue:
        for entry, sha, fmt in catalogue_ppc_archives(args.catalogue):
            try:
                targets.append((entry, fetch(sha, fmt, args.cache)))
            except Exception as error:  # noqa: BLE001 - report and continue
                print(f"skip {entry}: {error}", file=sys.stderr)

    reports = {}
    for title, archive in targets:
        try:
            report = audit(args.binary, archive)
        except Exception as error:  # noqa: BLE001 - report and continue
            print(f"audit failed for {title}: {error}", file=sys.stderr)
            continue
        if not report["fragments"]:
            continue
        reports[title] = report
        (args.out / f"{title}.json").write_text(json.dumps(report, indent=2) + "\n")

    summary = summarise(reports)
    (args.out / "summary.md").write_text(summary)
    print(args.out / "summary.md")


if __name__ == "__main__":
    main()
