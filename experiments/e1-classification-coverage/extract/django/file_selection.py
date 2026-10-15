"""The frozen Django file-selection rule of `inputs.md` §3."""

import fnmatch
from pathlib import PurePosixPath

# In the order of the frozen command line.
EXCLUDED_DIRECTORIES = (
    "migrations", "tests", "test", "node_modules", "venv", ".venv", "docs", "site-packages",
)  # fmt: skip
EXCLUDED_FILE_PATTERNS = ("test_*.py", "*_test.py", "conftest.py")


def is_excluded_path(relative_path: str) -> bool:
    path = PurePosixPath(relative_path)
    in_excluded_directory = not set(EXCLUDED_DIRECTORIES).isdisjoint(path.parent.parts)
    is_excluded_file = any(
        fnmatch.fnmatchcase(path.name, pattern) for pattern in EXCLUDED_FILE_PATTERNS
    )
    return in_excluded_directory or is_excluded_file


def excluded_globs() -> tuple[str, ...]:
    """The `--exclude` globs that realize the rule in the analyzer command."""
    return tuple(f"**/{name}/**" for name in EXCLUDED_DIRECTORIES) + tuple(
        f"**/{pattern}" for pattern in EXCLUDED_FILE_PATTERNS
    )
