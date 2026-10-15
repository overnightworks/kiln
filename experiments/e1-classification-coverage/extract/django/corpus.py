"""The corpus edge: manifest rows, where the clones live, and the commands that
fetch an application and install the pinned analyzer."""

import csv
import subprocess
from dataclasses import dataclass
from pathlib import Path

from file_selection import is_excluded_path

MANIFEST_PATH = Path(__file__).resolve().parents[2] / "corpus-manifest.csv"
DJANGO = "Django"
SPLINTER_REPOSITORY = "https://github.com/umd-dslam/splinter-mypy"
SPLINTER_COMMIT = "f8057cae6c2faae19e7cf97f3026a4fbebd4f9cc"
ANALYZER_PYTHON = "python3.11"
ANALYZER_DIRECTORY = "splinter-mypy-tools"


class CorpusError(Exception):
    """The corpus cannot be fetched as configured."""


@dataclass(frozen=True)
class ManifestRow:
    orm: str
    owner_repo: str
    repository_url: str
    commit_sha_full: str

    @property
    def directory_name(self) -> str:
        return self.owner_repo.replace("/", "__")


@dataclass(frozen=True)
class Command:
    arguments: tuple[str, ...]
    directory: Path


def read_manifest(path: Path) -> dict[str, ManifestRow]:
    with path.open(newline="", encoding="utf-8") as manifest:
        rows = (
            ManifestRow(r["orm"], r["owner_repo"], r["repository_url"], r["commit_sha_full"])
            for r in csv.DictReader(manifest)
        )
        return {row.owner_repo: row for row in rows}


def application_row(manifest: dict[str, ManifestRow], owner_repo: str) -> ManifestRow:
    row = manifest.get(owner_repo)
    if row is None or row.orm != DJANGO:
        raise CorpusError(f"{owner_repo} is not a Django application of the manifest")
    return row


def corpus_directory(setting: str | None, repository_root: Path) -> Path:
    if not setting:
        raise CorpusError("set the corpus directory (--corpus-dir or KILN_E1_CORPUS_DIR)")
    directory = Path(setting).resolve()
    if directory == repository_root or repository_root in directory.parents:
        raise CorpusError(f"{directory} is inside the repository; keep the corpus outside it")
    return directory


def application_directory(row: ManifestRow, corpus: Path) -> Path:
    return corpus / row.directory_name


def analyzer_python(corpus: Path) -> Path:
    return corpus / ANALYZER_DIRECTORY / "venv" / "bin" / "python"


def fetch_commands(row: ManifestRow, corpus: Path) -> tuple[Command, ...]:
    target = application_directory(row, corpus)
    return (
        Command(("git", "init", "--quiet", str(target)), corpus),
        Command(("git", "fetch", "--depth", "1", row.repository_url, row.commit_sha_full), target),
        Command(("git", "checkout", "FETCH_HEAD"), target),
    )


def analyzer_install_commands(corpus: Path) -> tuple[Command, ...]:
    tools = corpus / ANALYZER_DIRECTORY
    checkout = tools / "splinter-mypy"
    pip = tools / "venv" / "bin" / "pip"
    return (
        Command(("git", "clone", "--quiet", SPLINTER_REPOSITORY, str(checkout)), tools),
        Command(("git", "checkout", "--quiet", SPLINTER_COMMIT), checkout),
        Command((ANALYZER_PYTHON, "-m", "venv", str(tools / "venv")), tools),
        Command((str(pip), "install", "--quiet", "-r", str(checkout / "requirements.txt")), tools),
        Command((str(pip), "install", "--quiet", "--no-deps", "-e", str(checkout)), tools),
    )


def verify_snapshot(row: ManifestRow, application_root: Path) -> None:
    """The analyzer reads the filesystem, so the checkout must be exactly the manifest commit."""
    head = _git_output(application_root, "rev-parse", "HEAD").strip()
    if head != row.commit_sha_full:
        raise CorpusError(f"{application_root} is at {head}, not the manifest commit {row.commit_sha_full}")
    changed = _git_output(
        application_root, "status", "--porcelain", "-z", "--no-renames", "--ignored", "--untracked-files=all"
    )
    strays = [
        entry[3:] for entry in changed.split("\0")
        if entry[3:].endswith(".py") and not is_excluded_path(entry[3:])
    ]  # fmt: skip
    if strays:
        raise CorpusError(f"{application_root} differs from its commit in the selection: {sorted(strays)}")


def _git_output(directory: Path, *arguments: str) -> str:
    return subprocess.run(
        ["git", *arguments], cwd=directory, check=True, capture_output=True, text=True
    ).stdout


def run_commands(commands: tuple[Command, ...]) -> None:
    for command in commands:
        command.directory.mkdir(parents=True, exist_ok=True)
        subprocess.run(command.arguments, cwd=command.directory, check=True)
