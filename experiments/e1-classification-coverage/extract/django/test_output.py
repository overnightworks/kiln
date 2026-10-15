import json
import unittest

from unit_fixtures import APPLICATION, enumeration_of, marker, site
from rendering import render_units

SOURCE = """\
@transaction.atomic
def pay(order_id):
    Order.objects.get(pk=order_id)
    Ledger.objects.create(order=order_id)


def read(key):
    return Order.objects.first(), Order.objects.last()
"""


def messages():
    sites = [
        site(SOURCE, "Order.objects.get"),
        site(SOURCE, "Ledger.objects.create", method="create", method_type="write"),
        site(SOURCE, "Order.objects.first"),
        site(SOURCE, "Order.objects.last"),
    ]
    return sites, [marker(SOURCE, "transaction.atomic", name="app.pay")]


class OutputTests(unittest.TestCase):
    def test_kiln_e1dj_15_unit_is_one_json_line_in_the_shape_kiln_e1_reads(self):
        sites, markers = messages()
        lines = render_units(enumeration_of(SOURCE, sites, markers).units).splitlines()
        self.assertEqual(
            json.loads(lines[0]),
            {
                "subject": {"application": APPLICATION},
                "file": "app/views.py",
                "line": 2,
                "column": 0,
                "rule": "S2",
                "parameters": ["order_id"],
                "absorbed_sites": [
                    {"file": "app/views.py", "line": 3, "column": 4},
                    {"file": "app/views.py", "line": 4, "column": 4},
                ],
            },
        )
        self.assertEqual(len(lines), 3)

    def test_kiln_e1dj_16_order_and_bytes_do_not_depend_on_message_order(self):
        sites, markers = messages()
        forward = render_units(enumeration_of(SOURCE, sites, markers).units)
        backward = render_units(
            enumeration_of(SOURCE, sites[::-1], markers[::-1]).units
        )
        self.assertEqual(forward, backward)
        self.assertEqual(
            [json.loads(line)["line"] for line in forward.splitlines()], [2, 8, 8]
        )
        self.assertEqual(
            [json.loads(line)["column"] for line in forward.splitlines()], [0, 11, 34]
        )


if __name__ == "__main__":
    unittest.main()
