import unittest

from analyzer_command import frozen_analyzer_arguments
from analyzer_messages import analyzer_json, marker_message, model_message, site_message
from analyzer_report import AnalyzerReportError, parse_analyzer_json
from file_selection import is_excluded_path
from unit_fixtures import FILE, ROOT, enumeration_of, marker, site, units_of

SOURCE = """\
def view(request):
    Order.objects.get(pk=1)
    Order.objects.create()
    cursor.execute("select 1")
    return Order.objects.filter(a=1)
"""

FROZEN_ARGUMENTS = (
    "-m", "splinter", "shop",
    "--exclude", "**/migrations/**",
    "--exclude", "**/tests/**",
    "--exclude", "**/test/**",
    "--exclude", "**/node_modules/**",
    "--exclude", "**/venv/**",
    "--exclude", "**/.venv/**",
    "--exclude", "**/docs/**",
    "--exclude", "**/site-packages/**",
    "--exclude", "**/test_*.py",
    "--exclude", "**/*_test.py",
    "--exclude", "**/conftest.py",
    "--output", "messages.json",
)  # fmt: skip

SELECTION_CASES = [
    ("app/views.py", False),
    ("app/tests/test_views.py", True),
    ("tests/helpers.py", True),
    ("app/test/helpers.py", True),
    ("app/migrations/0001_initial.py", True),
    ("web/node_modules/x/y.py", True),
    ("venv/lib/x.py", True),
    ("app/.venv/lib/x.py", True),
    ("docs/conf.py", True),
    ("lib/site-packages/x.py", True),
    ("app/test_models.py", True),
    ("app/models_test.py", True),
    ("app/conftest.py", True),
    ("app/testing/helpers.py", False),
    ("app/contest.py", False),
    ("app/latest.py", False),
    ("app/documents/views.py", False),
]


class SiteTests(unittest.TestCase):
    def test_kiln_e1dj_01_sites_are_exactly_the_orm_method_messages(self):
        messages = [
            site_message("shop/a.py", SOURCE, "Order.objects.get"),
            site_message("shop/a.py", SOURCE, "Order.objects.create", "create", "write"),
            site_message("shop/a.py", SOURCE, "cursor.execute", "execute", "other"),
            model_message("shop/a.py", SOURCE, "def view"),
            marker_message("shop/a.py", SOURCE, "def view"),
        ]
        report = parse_analyzer_json(analyzer_json(*messages), ROOT)
        self.assertEqual(
            [
                (s.location.position.line, s.location.position.column, s.method)
                for s in report.sites
            ],
            [(2, 4, "get"), (3, 4, "create"), (4, 4, "execute")],
        )
        self.assertEqual(len(report.markers), 1)

    def test_kiln_e1dj_01_an_unknown_method_type_fails_loudly(self):
        message = site_message("shop/a.py", SOURCE, "Order", method_type="mystery")
        with self.assertRaisesRegex(AnalyzerReportError, "mystery"):
            parse_analyzer_json(analyzer_json(message), ROOT)

    def test_kiln_e1dj_02_analyzer_runs_with_the_one_frozen_command_line(self):
        self.assertEqual(
            frozen_analyzer_arguments("shop", "messages.json"), FROZEN_ARGUMENTS
        )

    def test_kiln_e1dj_03_excluded_paths_are_discarded_and_others_kept(self):
        for path, excluded in SELECTION_CASES:
            with self.subTest(path=path):
                self.assertEqual(is_excluded_path(path), excluded)

    def test_kiln_e1dj_03_messages_from_excluded_files_never_reach_enumeration(self):
        kept = site_message(f"{ROOT}/app/views.py", SOURCE, "Order.objects.get")
        dropped = site_message(f"{ROOT}/app/tests/test_views.py", SOURCE, "Order.objects.get")
        report = parse_analyzer_json(analyzer_json(kept, dropped), ROOT)
        self.assertEqual([s.location.file for s in report.sites], ["app/views.py"])

    def test_kiln_e1dj_20_messages_of_followed_modules_outside_the_root_are_discarded(self):
        inside = site_message(f"{ROOT}/app/views.py", SOURCE, "Order.objects.get")
        outside = site_message("/venv/site-packages/lib.py", SOURCE, "Order.objects.get")
        sibling = site_message(f"{ROOT}-other/app/views.py", SOURCE, "Order.objects.get")
        report = parse_analyzer_json(analyzer_json(inside, outside, sibling), ROOT)
        self.assertEqual([s.location.file for s in report.sites], ["app/views.py"])

    def test_kiln_e1dj_13_marker_that_is_no_decorator_or_with_fails_loudly(self):
        sites = [site(SOURCE, "Order.objects.get")]
        markers = [marker(SOURCE, "Order.objects.create")]
        with self.assertRaisesRegex(AnalyzerReportError, FILE):
            units_of(SOURCE, sites, markers)

    def test_kiln_e1dj_13_site_beyond_the_source_text_fails_loudly(self):
        beyond = site(SOURCE, "Order.objects.get")
        beyond["fromLine"] = 99
        with self.assertRaisesRegex(AnalyzerReportError, FILE):
            units_of(SOURCE, [beyond])

    def test_kiln_e1dj_13_site_in_a_file_without_source_fails_loudly(self):
        report = parse_analyzer_json(
            analyzer_json(site_message("shop/gone.py", SOURCE, "Order")), ROOT
        )
        from enumeration import enumerate_units

        with self.assertRaisesRegex(AnalyzerReportError, "gone.py"):
            enumerate_units("acme/shop", report, {})


class ReceiverEvidenceTests(unittest.TestCase):
    def test_kiln_e1dj_14_any_receiver_is_marked_unresolved_and_no_unit_is_dropped(self):
        sites = [
            site(SOURCE, "Order.objects.get", receiver="request.POST", receiver_types=("Any",)),
            site(SOURCE, "Order.objects.create", method="create", method_type="write"),
        ]
        enumeration = enumeration_of(SOURCE, sites)
        self.assertEqual(len(enumeration.units), 2)
        self.assertEqual(
            [
                (c.location.position.line, c.method, c.receiver, c.receiver_types, c.receiver_resolved)
                for c in enumeration.receiver_evidence
            ],
            [
                (2, "get", "request.POST", ("Any",), False),
                (3, "create", "objects", ("django.db.models.manager.Manager[app.Item]",), True),
            ],
        )


if __name__ == "__main__":
    unittest.main()
