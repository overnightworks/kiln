import unittest
from pathlib import Path

from corpus import (
    MANIFEST_PATH,
    SPLINTER_COMMIT,
    CorpusError,
    analyzer_install_commands,
    application_row,
    corpus_directory,
    fetch_commands,
    read_manifest,
)

REPOSITORY_ROOT = Path("/work/kiln")
CORPUS = Path("/data/corpus")
BABYBUDDY_COMMIT = "ba46c49bea0694fd6b9bda728bcd281ef15f284c"


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

    def test_kiln_e1dj_19_the_pinned_analyzer_environment_is_installed_in_the_corpus(self):
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
