import unittest

from analyzer_report import AnalyzerReportError
from unit_fixtures import FILE, definition_marker, enumeration_of, marker, shape, site, units_of
from unit_records import SiteLocation, SourcePosition

DECORATED = """\
from django.db import transaction


@transaction.atomic(savepoint=False)
def reactivate(user, *, acting):
    Log.objects.create(user=user)
    user.save()


@transaction.atomic
def bare(order_id):
    Order.objects.get(pk=order_id)


@transaction.atomic
def empty(order_id):
    helper(order_id)
"""

WITH_BLOCK = """\
def pay(order_id, amount):
    first = Order.objects.get(pk=order_id)
    with transaction.atomic(using=Router.objects.first()):
        first.save()
        Ledger.objects.create(amount=amount)
    Audit.objects.create(order=first)
"""

NESTED = """\
@transaction.atomic
def outer(a):
    Order.objects.get(pk=a)
    with transaction.atomic():
        Order.objects.create(pk=a)
        with transaction.atomic():
            Order.objects.first()
    inner_helper(a)

@transaction.atomic
def sibling(b):
    Order.objects.last()
"""

HELPERS = """\
def helper(order_id):
    return Order.objects.get(pk=order_id)


@transaction.atomic
def pay(order_id):
    helper(order_id)

    def closure():
        return Order.objects.first()

    closure()
"""

TWO_SITES = """\
def view(request, key):
    return Order.objects.get(pk=key), Order.objects.count()
"""

BRANCHES = """\
def view(flag, rows):
    if flag:
        a = Order.objects.get(pk=1)
    elif Order.objects.exists():
        a = None
    b = [Order.objects.first() for _ in rows]
    for row in rows:
        Order.objects.create(row=row)
    return Order.objects.last() if flag else a, b
"""

PARAMETERS = """\
class Views:
    def post(self, request, *args, flag=None, **kwargs):
        Order.objects.create()
        first = request.GET
        unused = 5
        for item in first:
            pass
        Order.objects.get(pk=first, who=request.user, flag=flag, other=GLOBAL)
        first = 2
        later = Order.objects.filter(owner=self, item=item, again=later)
        return later

    def get(self):
        def inner(z):
            return Order.objects.first(z)
        return inner


Order.objects.all(GLOBAL)
"""

FSTRING = """\
def show(user, other):
    print(f"{user.profile.count()} of {other}")
"""

TWO_CALLS_IN_ONE_FSTRING = """\
def view(left, right):
    return f"{left.get()} {right.get()}"
"""

NESTED_IN_DECORATOR = """\
def other(function):
    return function


@transaction.atomic() and other
def view(a, b):
    a.get()
    b.get()
"""

UNREPORTED = """\
def get(self, request):
    return super(View, self).get(request)
"""


ALIASED = """\
from django.db.transaction import atomic as tx


@other
@tx
def aliased(a):
    Order.objects.get(pk=a)


@tx
@other
def stacked(b):
    Order.objects.last(b)
"""

DECORATED_FIRST_STATEMENT = """\
def run(a):
    with transaction.atomic():
        @decorate(Order.objects.get(pk=a))
        def inner():
            pass
"""

EXIT_STACK = """\
def run(stack):
    stack.enter_context(transaction.atomic())
    Order.objects.get()
"""


class TransactionUnitTests(unittest.TestCase):
    def test_kiln_e1dj_04_decorated_function_is_one_s2_unit_with_its_parameters(self):
        sites = [
            site(DECORATED, "Log.objects.create", method="create", method_type="write"),
            site(DECORATED, "user.save", method="save", method_type="write"),
            site(DECORATED, "Order.objects.get"),
        ]
        markers = [
            marker(DECORATED, "transaction.atomic(savepoint=False)"),
            definition_marker(DECORATED, "transaction.atomic\ndef bare", "Order.objects.get(pk=order_id)", "app.bare"),
            definition_marker(DECORATED, "transaction.atomic\ndef empty", "helper(order_id)", "app.empty"),
        ]
        units = units_of(DECORATED, sites, markers)
        self.assertEqual(
            [shape(unit) for unit in units],
            [
                ("S2", 5, ("user", "acting"), (6, 7)),
                ("S2", 11, ("order_id",), (12,)),
                ("S2", 16, ("order_id",), ()),
            ],
        )

    def test_kiln_e1dj_04_aliased_and_stacked_decorators_are_matched_by_the_analyzers_report(self):
        sites = [site(ALIASED, "Order.objects.get"), site(ALIASED, "Order.objects.last")]
        markers = [
            definition_marker(ALIASED, "other\n@tx\ndef aliased", "Order.objects.get(pk=a)", "app.aliased"),
            definition_marker(ALIASED, "tx\n@other", "Order.objects.last(b)", "app.stacked"),
        ]
        units = units_of(ALIASED, sites, markers)
        self.assertEqual(
            [shape(unit) for unit in units],
            [("S2", 6, ("a",), (7,)), ("S2", 12, ("b",), (13,))],
        )

    def test_kiln_e1dj_05_decorator_of_the_first_statement_is_part_of_the_body(self):
        sites = [site(DECORATED_FIRST_STATEMENT, "Order.objects.get")]
        markers = [marker(DECORATED_FIRST_STATEMENT, "transaction.atomic")]
        units = units_of(DECORATED_FIRST_STATEMENT, sites, markers)
        self.assertEqual([shape(unit) for unit in units], [("S2", 2, ("a",), (3,))])

    def test_kiln_e1dj_22_marker_that_is_no_decorator_or_with_opens_no_unit_and_is_recorded(self):
        sites = [site(EXIT_STACK, "Order.objects.get")]
        enumeration = enumeration_of(EXIT_STACK, sites, [marker(EXIT_STACK, "transaction.atomic")])
        self.assertEqual([shape(unit) for unit in enumeration.units], [("S3", 3, (), (3,))])
        self.assertEqual(enumeration.unmatched_markers, [SiteLocation(FILE, SourcePosition(2, 24))])

    def test_kiln_e1dj_22_marker_nested_in_a_larger_decorator_expression_opens_no_unit(self):
        sites = [site(NESTED_IN_DECORATOR, "a.get", receiver="a"), site(NESTED_IN_DECORATOR, "b.get", receiver="b")]
        nested_marker = marker(NESTED_IN_DECORATOR, "transaction.atomic()")
        enumeration = enumeration_of(NESTED_IN_DECORATOR, sites, [nested_marker])
        self.assertEqual(
            [shape(unit) for unit in enumeration.units], [("S3", 7, ("a",), (7,)), ("S3", 8, ("b",), (8,))]
        )
        self.assertEqual(enumeration.unmatched_markers, [SiteLocation(FILE, SourcePosition(5, 1))])

    def test_kiln_e1dj_05_with_block_is_one_s2_unit_of_the_enclosing_function(self):
        sites = [
            site(WITH_BLOCK, "Order.objects.get"),
            site(WITH_BLOCK, "Router.objects.first"),
            site(WITH_BLOCK, "first.save"),
            site(WITH_BLOCK, "Ledger.objects.create"),
            site(WITH_BLOCK, "Audit.objects.create"),
        ]
        units = units_of(WITH_BLOCK, sites, [marker(WITH_BLOCK, "transaction.atomic")])
        self.assertEqual(
            [shape(unit) for unit in units],
            [
                ("S3", 2, ("order_id",), (2,)),
                ("S2", 3, ("order_id", "amount"), (4, 5)),
                ("S3", 3, (), (3,)),
                ("S3", 6, ("first",), (6,)),
            ],
        )

    def test_kiln_e1dj_06_outermost_transaction_owns_nested_ones(self):
        sites = [
            site(NESTED, "Order.objects.get"),
            site(NESTED, "Order.objects.create"),
            site(NESTED, "Order.objects.first"),
            site(NESTED, "Order.objects.last"),
        ]
        markers = [
            definition_marker(NESTED, "transaction.atomic\ndef outer", "inner_helper(a)", "app.outer"),
            marker(NESTED, "transaction.atomic()", occurrence=1),
            marker(NESTED, "transaction.atomic()", occurrence=2),
            definition_marker(NESTED, "transaction.atomic\ndef sibling", "Order.objects.last()", "app.sibling"),
        ]
        units = units_of(NESTED, sites, markers)
        self.assertEqual(
            [shape(unit) for unit in units],
            [("S2", 2, ("a",), (3, 5, 7)), ("S2", 11, ("b",), (12,))],
        )

    def test_kiln_e1dj_07_helpers_are_not_expanded_but_closures_are_absorbed(self):
        sites = [
            site(HELPERS, "Order.objects.get"),
            site(HELPERS, "Order.objects.first"),
        ]
        markers = [definition_marker(HELPERS, "transaction.atomic", "\n    closure()", "app.pay")]
        units = units_of(HELPERS, sites, markers)
        self.assertEqual(
            [shape(unit) for unit in units],
            [("S3", 2, ("order_id",), (2,)), ("S2", 6, ("order_id",), (10,))],
        )


class AutocommitUnitTests(unittest.TestCase):
    def test_kiln_e1dj_08_site_outside_transactions_is_its_own_s3_unit(self):
        sites = [site(TWO_SITES, "Order.objects.get")]
        (unit,) = units_of(TWO_SITES, sites)
        self.assertEqual(
            (unit.rule.value, unit.location.file, unit.location.position.line),
            ("S3", FILE, 2),
        )
        self.assertEqual(unit.location.position.column, 11)
        self.assertEqual(unit.absorbed_sites, (unit.location,))

    def test_kiln_e1dj_09_statement_with_two_sites_yields_two_units(self):
        sites = [
            site(TWO_SITES, "Order.objects.get"),
            site(TWO_SITES, "Order.objects.count", method="count"),
        ]
        units = units_of(TWO_SITES, sites)
        self.assertEqual(
            [(unit.location.position.line, unit.location.position.column) for unit in units],
            [(2, 11), (2, 38)],
        )

    def test_kiln_e1dj_10_conditional_comprehension_and_loop_sites_count_once(self):
        needles = [
            "Order.objects.get",
            "Order.objects.exists",
            "Order.objects.first",
            "Order.objects.create",
            "Order.objects.last",
        ]
        units = units_of(BRANCHES, [site(BRANCHES, needle) for needle in needles])
        self.assertEqual(
            [unit.location.position.line for unit in units], [3, 4, 6, 8, 9]
        )

    def test_kiln_e1dj_11_parameters_are_the_declared_and_earlier_bound_names_the_call_reads(self):
        needles = [
            "Order.objects.create",
            "Order.objects.get",
            "Order.objects.filter",
            "Order.objects.first",
            "Order.objects.all",
        ]
        units = units_of(PARAMETERS, [site(PARAMETERS, needle) for needle in needles])
        self.assertEqual(
            [unit.parameters for unit in units],
            [(), ("request", "flag", "first"), ("self", "item"), ("z",), ()],
        )

    def test_kiln_e1dj_11_call_inside_an_fstring_is_read_where_the_analyzer_reports_it(self):
        reported_at_the_fstring = site(FSTRING, 'f"{user', method="count", end_after="count()")
        (unit,) = units_of(FSTRING, [reported_at_the_fstring])
        self.assertEqual(unit.parameters, ("user",))

    def test_kiln_e1dj_11_calls_reported_at_the_same_start_keep_their_own_parameters(self):
        sites = [
            site(TWO_CALLS_IN_ONE_FSTRING, 'f"{left', receiver="left", method="get", end_after="left.get()"),
            site(TWO_CALLS_IN_ONE_FSTRING, 'f"{left', receiver="right", method="get", end_after="right.get()"),
        ]
        units = units_of(TWO_CALLS_IN_ONE_FSTRING, sites)
        self.assertEqual([unit.parameters for unit in units], [("left",), ("right",)])

    def test_kiln_e1dj_11_call_the_reported_span_does_not_single_out_fails_loudly(self):
        unplaced = site(FSTRING, 'f"{user', method="count", end_after="other")
        with self.assertRaisesRegex(AnalyzerReportError, "0 calls of `count`"):
            units_of(FSTRING, [unplaced])

    def test_kiln_e1dj_12_call_the_analyzer_does_not_report_yields_no_unit(self):
        self.assertEqual(units_of(UNREPORTED, []), [])


if __name__ == "__main__":
    unittest.main()
