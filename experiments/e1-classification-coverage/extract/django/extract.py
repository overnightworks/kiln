"""Command line of the Django unit extractor.

    extract.py install-analyzer
    extract.py fetch OWNER/REPO
    extract.py units OWNER/REPO --output-dir DIR

The corpus directory comes from `--corpus-dir` or `KILN_E1_CORPUS_DIR` and must
lie outside the repository.
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

from analyzer_command import run_analyzer
from analyzer_report import parse_analyzer_json
from corpus import (
    MANIFEST_PATH,
    ManifestRow,
    analyzer_install_commands,
    analyzer_python,
    application_directory,
    application_row,
    corpus_directory,
    fetch_commands,
    read_manifest,
    run_commands,
    verify_snapshot,
)
from enumeration import enumerate_units
from rendering import render_receiver_evidence, render_units, render_unmatched_markers

CORPUS_ENVIRONMENT_VARIABLE = "KILN_E1_CORPUS_DIR"


def main(arguments: list[str]) -> None:
    parsed = _parser().parse_args(arguments)
    corpus = corpus_directory(parsed.corpus_dir, _repository_root())
    if parsed.command == "install-analyzer":
        run_commands(analyzer_install_commands(corpus))
        return
    row = application_row(read_manifest(MANIFEST_PATH), parsed.application)
    if parsed.command == "fetch":
        run_commands(fetch_commands(row, corpus))
    else:
        _write_units(row, corpus, parsed.output_dir)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus-dir", default=os.environ.get(CORPUS_ENVIRONMENT_VARIABLE))
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("install-analyzer")
    commands.add_parser("fetch").add_argument("application")
    units = commands.add_parser("units")
    units.add_argument("application")
    units.add_argument("--output-dir", type=Path, required=True)
    return parser


def _repository_root() -> Path:
    toplevel = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        cwd=Path(__file__).parent, check=True, capture_output=True, text=True,
    ).stdout.strip()  # fmt: skip
    return Path(toplevel).resolve()


def _write_units(row: ManifestRow, corpus: Path, output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    stem = row.directory_name
    messages = output_dir / f"{stem}.analyzer.json"
    root = application_directory(row, corpus)
    verify_snapshot(row, root)
    run_analyzer(analyzer_python(corpus), corpus, str(root), messages)
    report = parse_analyzer_json(messages.read_text(encoding="utf-8"), str(root))
    sources = {
        file: (root / file).read_text(encoding="utf-8")
        for file in sorted({site.location.file for site in report.sites} | {m.file for m in report.markers})
    }  # fmt: skip
    enumeration = enumerate_units(row.owner_repo, report, sources)
    (output_dir / f"{stem}.units.jsonl").write_text(render_units(enumeration.units), encoding="utf-8")
    (output_dir / f"{stem}.receivers.jsonl").write_text(
        render_receiver_evidence(enumeration.receiver_evidence), encoding="utf-8"
    )
    (output_dir / f"{stem}.unmatched-markers.jsonl").write_text(
        render_unmatched_markers(enumeration.unmatched_markers), encoding="utf-8"
    )


if __name__ == "__main__":
    main(sys.argv[1:])
