"""The one frozen `splinter-mypy` command line of `inputs.md` §3, and the edge
that runs it."""

import subprocess
from pathlib import Path

from file_selection import excluded_globs


def frozen_analyzer_arguments(application_root: str, output: str) -> tuple[str, ...]:
    exclusions = [part for glob in excluded_globs() for part in ("--exclude", glob)]
    return ("-m", "splinter", application_root, *exclusions, "--output", output)


def run_analyzer(
    analyzer_python: Path, working_directory: Path, application_root: str, output: Path
) -> None:
    arguments = frozen_analyzer_arguments(application_root, str(output))
    subprocess.run(
        [str(analyzer_python), *arguments],
        cwd=working_directory,
        check=True,
        stdout=subprocess.DEVNULL,
    )
