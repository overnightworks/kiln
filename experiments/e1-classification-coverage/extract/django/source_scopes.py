"""What the source text of one file says: explicit transaction blocks, and the
function scope around a position."""

import ast
from dataclasses import dataclass
from typing import Iterator, Mapping

from analyzer_report import AnalyzerReportError
from unit_records import SourcePosition

FUNCTION_NODES = (ast.FunctionDef, ast.AsyncFunctionDef)
NESTED_SCOPE_NODES = (
    *FUNCTION_NODES, ast.Lambda, ast.ClassDef,
    ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp,
)  # fmt: skip
ASSIGNMENT_NODES = (ast.Assign, ast.AnnAssign, ast.AugAssign)


@dataclass(frozen=True)
class Span:
    start: SourcePosition
    end: SourcePosition

    def contains(self, position: SourcePosition) -> bool:
        return self.start <= position < self.end


@dataclass(frozen=True)
class FunctionScope:
    body: Span
    parameters: tuple[str, ...]
    bindings: tuple[tuple[SourcePosition, str], ...]  # (available from, name)

    def feeding(self, position: SourcePosition, referenced: frozenset[str]) -> tuple[str, ...]:
        """Declared parameters, then locals bound before `position`, that `referenced` names."""
        earlier = [name for available, name in self.bindings if available <= position]
        return tuple(name for name in dict.fromkeys((*self.parameters, *earlier)) if name in referenced)


@dataclass(frozen=True)
class TransactionBlock:
    position: SourcePosition
    body: Span
    parameters: tuple[str, ...]


@dataclass(frozen=True)
class SourceScopes:
    line_widths: tuple[int, ...]  # bytes per line, as the parser counts columns
    functions: tuple[FunctionScope, ...]
    transactions: tuple[TransactionBlock, ...]
    unmatched_markers: tuple[SourcePosition, ...]
    calls_by_position: Mapping[SourcePosition, tuple[ast.Call, ...]]

    def contains(self, position: SourcePosition) -> bool:
        in_lines = 1 <= position.line <= len(self.line_widths)
        return in_lines and 0 <= position.column < self.line_widths[position.line - 1]

    def names_in_call(self, position: SourcePosition, method: str) -> frozenset[str]:
        """The names the reported call reads, receiver and arguments."""
        reported = [c for c in self.calls_by_position.get(position, ()) if _calls_method(c, method)]
        if not reported:
            raise AnalyzerReportError(f"no call of `{method}` at {position}")
        return frozenset(n.id for call in reported for n in ast.walk(call) if isinstance(n, ast.Name))

    def function_at(self, position: SourcePosition) -> FunctionScope | None:
        containing = [f for f in self.functions if f.body.contains(position)]
        return max(containing, key=lambda scope: scope.body.start, default=None)


def read_scopes(file: str, text: str, marker_positions: frozenset[SourcePosition]) -> SourceScopes:
    tree = ast.parse(text, filename=file)
    nodes = list(ast.walk(tree))
    functions = tuple(_function_scope(node) for node in nodes if isinstance(node, FUNCTION_NODES))
    line_widths = tuple(len(line.encode("utf-8")) for line in text.split("\n"))
    calls = _calls_by_position(nodes)
    scopes = SourceScopes(line_widths, functions, (), (), calls)
    for marker in sorted(marker_positions):
        if not scopes.contains(marker):
            raise AnalyzerReportError(f"{file}: transaction marker at {marker} is outside the source text")
    blocks, consumed = _transaction_blocks(nodes, scopes, marker_positions)
    unmatched = tuple(sorted(marker_positions - consumed))
    return SourceScopes(line_widths, functions, tuple(blocks), unmatched, calls)


def _position(node: ast.AST) -> SourcePosition:
    return SourcePosition(node.lineno, node.col_offset)


def _calls_by_position(nodes: list[ast.AST]) -> dict[SourcePosition, tuple[ast.Call, ...]]:
    """Calls by where the analyzer reports them: their start, or, inside an f-string,
    the start of the f-string, which is where mypy places every expression of it."""
    grouped: dict[SourcePosition, list[ast.Call]] = {}
    for node in nodes:
        if isinstance(node, ast.Call):
            grouped.setdefault(_position(node), []).append(node)
        elif isinstance(node, ast.JoinedStr):
            inside = [n for n in ast.walk(node) if isinstance(n, ast.Call)]
            grouped.setdefault(_position(node), []).extend(inside)
    return {position: tuple(calls) for position, calls in grouped.items()}


def _statement_start(statement: ast.stmt) -> SourcePosition:
    """A decorated definition starts at its first decorator, not at `def`."""
    decorators = getattr(statement, "decorator_list", [])
    return min((_position(d) for d in decorators), default=_position(statement))


def _body_span(body: list[ast.stmt]) -> Span:
    return Span(_statement_start(body[0]), SourcePosition(body[-1].end_lineno, body[-1].end_col_offset))


def _declared_parameters(arguments: ast.arguments) -> tuple[str, ...]:
    declared = [*arguments.posonlyargs, *arguments.args]
    if arguments.vararg:
        declared.append(arguments.vararg)
    declared.extend(arguments.kwonlyargs)
    if arguments.kwarg:
        declared.append(arguments.kwarg)
    return tuple(argument.arg for argument in declared)


def _function_scope(node: ast.FunctionDef | ast.AsyncFunctionDef) -> FunctionScope:
    bindings = sorted(_bindings(node.body), key=lambda binding: binding[0])
    return FunctionScope(_body_span(node.body), _declared_parameters(node.args), tuple(bindings))


def _walk_own_scope(statements: list[ast.stmt]) -> Iterator[ast.AST]:
    pending: list[ast.AST] = list(statements)
    while pending:
        node = pending.pop()
        yield node
        if not isinstance(node, NESTED_SCOPE_NODES):
            pending.extend(ast.iter_child_nodes(node))


def _bindings(statements: list[ast.stmt]) -> Iterator[tuple[SourcePosition, str]]:
    """Names stored in the function itself; an assignment binds after its statement."""
    available_after: dict[int, SourcePosition] = {}
    nodes = list(_walk_own_scope(statements))
    for node in nodes:
        if isinstance(node, ASSIGNMENT_NODES):
            end = SourcePosition(node.end_lineno, node.end_col_offset)
            targets = node.targets if isinstance(node, ast.Assign) else [node.target]
            for target in targets:
                for stored in ast.walk(target):
                    available_after[id(stored)] = end
    for node in nodes:
        if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Store):
            yield available_after.get(id(node), _end(node)), node.id


def _end(node: ast.expr) -> SourcePosition:
    return SourcePosition(node.end_lineno, node.end_col_offset)


def _calls_method(call: ast.Call, method: str) -> bool:
    return isinstance(call.func, ast.Attribute) and call.func.attr == method


def _transaction_blocks(
    nodes: list[ast.AST], scopes: SourceScopes, markers: frozenset[SourcePosition]
) -> tuple[list[TransactionBlock], set[SourcePosition]]:
    blocks: list[TransactionBlock] = []
    consumed: set[SourcePosition] = set()
    for node in nodes:
        if isinstance(node, FUNCTION_NODES):
            matched = {_position(d) for d in node.decorator_list} & markers
            if matched:
                consumed |= matched
                blocks.append(TransactionBlock(_position(node), _body_span(node.body), _declared_parameters(node.args)))
        elif isinstance(node, ast.With):
            matched = {_position(item.context_expr) for item in node.items} & markers
            if matched:
                consumed |= matched
                enclosing = scopes.function_at(_position(node))
                parameters = enclosing.parameters if enclosing else ()
                blocks.append(TransactionBlock(_position(node), _body_span(node.body), parameters))
    return blocks, consumed
