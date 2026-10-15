import unittest
from pathlib import Path

from analyzer_report import parse_analyzer_json
from enumeration import enumerate_units
from unit_records import Unit, UnitRule

FIXTURES = Path(__file__).parent / "fixtures"
TIMER_ADD_QUICK_POST_LINES = range(456, 471)
DO_REACTIVATE_USER_DEF_LINE = 647


def units_recorded_for(application: str, source_file: str) -> list[Unit]:
    recorded = FIXTURES / application
    report = parse_analyzer_json((recorded / "messages.json").read_text(), application)
    source = (recorded / source_file).read_text()
    return enumerate_units(f"fixture/{application}", report, {source_file: source}).units


class WorkedExampleTests(unittest.TestCase):
    def test_kiln_e1dj_17_timer_add_quick_post_yields_seven_s3_units(self):
        units = [
            unit
            for unit in units_recorded_for("babybuddy", "core/views.py")
            if unit.location.position.line in TIMER_ADD_QUICK_POST_LINES
        ]
        self.assertEqual(
            [(unit.rule, unit.location.position.line) for unit in units],
            [(UnitRule.S3, line) for line in (457, 459, 460, 464, 465, 466, 467)],
        )

    def test_kiln_e1dj_18_do_reactivate_user_yields_one_s2_unit(self):
        units = [
            unit
            for unit in units_recorded_for("zulip", "zerver/actions/create_user.py")
            if unit.location.position.line >= DO_REACTIVATE_USER_DEF_LINE
        ]
        (unit,) = units
        self.assertEqual(unit.rule, UnitRule.S2)
        self.assertEqual(unit.parameters, ("user_profile", "acting_user"))
        self.assertEqual(
            [site.position.line for site in unit.absorbed_sites], [656, 677, 678, 724, 724, 727]
        )


if __name__ == "__main__":
    unittest.main()
