import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from corpus import (
    MANIFEST_PATH,
    SPLINTER_COMMIT,
    CorpusError,
    ManifestRow,
    analyzer_install_commands,
    application_row,
    corpus_directory,
    fetch_commands,
    read_manifest,
    verify_snapshot,
)

REPOSITORY_ROOT = Path("/work/kiln")
CORPUS = Path("/data/corpus")
BABYBUDDY_COMMIT = "ba46c49bea0694fd6b9bda728bcd281ef15f284c"
GIT_IDENTITY = {
    "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@example.com",
    "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@example.com",
}  # fmt: skip


def git(directory: Path, *arguments: str) -> str:
    result = subprocess.run(
        ["git", *arguments], cwd=directory, check=True, capture_output=True, text=True,
        env={**os.environ, **GIT_IDENTITY},
    )  # fmt: skip
    return result.stdout.strip()


def write(root: Path, relative: str, text: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        git(self.root, "init", "--quiet")
        write(self.root, "app/views.py", "x = 1\n")
        write(self.root, "app/tests/test_views.py", "x = 1\n")
        git(self.root, "add", ".")
        git(self.root, "commit", "--quiet", "-m", "pinned")
        self.row = ManifestRow("Django", "a/b", "https://example.invalid/a/b", git(self.root, "rev-parse", "HEAD"))

    def test_kiln_e1dj_21_checkout_at_the_manifest_commit_without_changes_is_accepted(self):
        verify_snapshot(self.row, self.root)

    def test_kiln_e1dj_21_checkout_at_another_commit_is_rejected(self):
        write(self.root, "app/more.py", "y = 2\n")
        git(self.root, "add", ".")
        git(self.root, "commit", "--quiet", "-m", "later")
        with self.assertRaisesRegex(CorpusError, self.row.commit_sha_full):
            verify_snapshot(self.row, self.root)

    def test_kiln_e1dj_21_python_sources_changed_or_added_in_the_selection_are_rejected(self):
        changes = {
            "modified": lambda: write(self.root, "app/views.py", "x = 2\n"),
            "deleted": lambda: (self.root / "app/views.py").unlink(),
            "untracked": lambda: write(self.root, "app/extra.py", "z = 3\n"),
            "ignored": lambda: (
                write(self.root, ".gitignore", "ignored.py\n"),
                write(self.root, "ignored.py", "z = 3\n"),
            ),
        }
        for name, change in changes.items():
            with self.subTest(change=name):
                change()
                with self.assertRaises(CorpusError):
                    verify_snapshot(self.row, self.root)
                git(self.root, "checkout", "--quiet", "--", ".")
                git(self.root, "clean", "--quiet", "-fdx")

    def test_kiln_e1dj_21_changes_outside_the_selection_are_accepted(self):
        write(self.root, "app/tests/test_views.py", "x = 2\n")
        write(self.root, "app/migrations/0001.py", "z = 3\n")
        write(self.root, "app/notes.txt", "n\n")
        verify_snapshot(self.row, self.root)


class CorpusTests(unittest.TestCase):
    def setUp(self):
        self.manifest = read_manifest(MANIFEST_PATH)

    def test_kiln_e1dj_19_fetch_clones_the_manifest_commit_into_the_corpus_directory(self):
        commands = fetch_commands(application_row(self.manifest, "babybuddy/babybuddy"), CORPUS)
        arguments = [command.arguments for command in commands]
        self.assertIn(
            ("git", "fetch", "--depth", "1", "https://github.com/babybuddy/babybuddy", BABYBUDDY_COMMIT),
            arguments,
        )
        self.assertIn(("git", "checkout", "FETCH_HEAD"), arguments)
        self.assertEqual({command.directory for command in commands[1:]}, {CORPUS / "babybuddy__babybuddy"})

    def test_kiln_e1dj_19_the_pinned_analyzer_environment_is_installed_by_its_own_command(self):
        commands = analyzer_install_commands(CORPUS)
        flattened = [part for command in commands for part in command.arguments]
        self.assertIn(SPLINTER_COMMIT, flattened)
        self.assertIn("--no-deps", flattened)
        self.assertIn("python3.11", flattened)
        self.assertTrue(
            all(CORPUS in (command.directory, *command.directory.parents) for command in commands)
        )

    def test_kiln_e1dj_19_unknown_application_is_rejected(self):
        with self.assertRaisesRegex(CorpusError, "nobody/nothing"):
            application_row(self.manifest, "nobody/nothing")

    def test_kiln_e1dj_19_application_of_another_orm_is_rejected(self):
        with self.assertRaisesRegex(CorpusError, "cool-team-official/cool-admin-midway"):
            application_row(self.manifest, "cool-team-official/cool-admin-midway")

    def test_kiln_e1dj_19_corpus_directory_must_be_configured_and_outside_the_repository(self):
        self.assertEqual(corpus_directory("/data/corpus", REPOSITORY_ROOT), CORPUS)
        for setting in (None, "", "/work/kiln/corpus", "/work/kiln"):
            with self.subTest(setting=setting):
                with self.assertRaises(CorpusError):
                    corpus_directory(setting, REPOSITORY_ROOT)


if __name__ == "__main__":
    unittest.main()
