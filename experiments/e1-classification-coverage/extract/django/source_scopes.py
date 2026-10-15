"""What the source text of one file says: explicit transaction blocks, and the
function scope around a position."""

import ast
from dataclasses import dataclass
from typing import Iterator

from analyzer_report import AnalyzerReportError
from unit_records import SourcePosition

ATOMIC_NAME = "atomic"
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

    def parameters_at(self, position: SourcePosition) -> tuple[str, ...]:
        earlier = [name for available, name in self.bindings if available <= position]
        return tuple(dict.fromkeys((*self.parameters, *earlier)))


@dataclass(frozen=True)
class TransactionBlock:
    position: SourcePosition
    body: Span
    parameters: tuple[str, ...]


@dataclass(frozen=True)
class SourceScopes:
    line_count: int
    functions: tuple[FunctionScope, ...]
    transactions: tuple[TransactionBlock, ...]

    def function_at(self, position: SourcePosition) -> FunctionScope | None:
        containing = [f for f in self.functions if f.body.contains(position)]
        return max(containing, key=lambda scope: scope.body.start, default=None)


def read_scopes(file: str, text: str, marker_positions: frozenset[SourcePosition]) -> SourceScopes:
    tree = ast.parse(text, filename=file)
    functions = tuple(_function_scope(node) for node in ast.walk(tree) if isinstance(node, FUNCTION_NODES))
    scopes = SourceScopes(text.count("\n") + 1, functions, ())
    blocks, consumed = _transaction_blocks(tree, scopes, marker_positions)
    unmatched = marker_positions - consumed
    if unmatched:
        raise AnalyzerReportError(
            f"{file}: transaction marker is neither a decorator nor a with block at {sorted(unmatched)}"
        )
    return SourceScopes(scopes.line_count, functions, tuple(blocks))


def _position(node: ast.AST) -> SourcePosition:
    return SourcePosition(node.lineno, node.col_offset)


def _body_span(body: list[ast.stmt]) -> Span:
    return Span(_position(body[0]), SourcePosition(body[-1].end_lineno, body[-1].end_col_offset))


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


def _is_atomic_reference(node: ast.expr) -> bool:
    return (isinstance(node, ast.Name) and node.id == ATOMIC_NAME) or (
        isinstance(node, ast.Attribute) and node.attr == ATOMIC_NAME
    )


def _transaction_blocks(
    tree: ast.Module, scopes: SourceScopes, markers: frozenset[SourcePosition]
) -> tuple[list[TransactionBlock], set[SourcePosition]]:
    blocks: list[TransactionBlock] = []
    consumed: set[SourcePosition] = set()
    for node in ast.walk(tree):
        if isinstance(node, FUNCTION_NODES):
            matched = _decorator_markers(node, markers)
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


def _decorator_markers(
    function: ast.FunctionDef | ast.AsyncFunctionDef, markers: frozenset[SourcePosition]
) -> set[SourcePosition]:
    """Marker positions that make this function an explicit transaction."""
    called = {_position(d) for d in function.decorator_list if isinstance(d, ast.Call)}
    matched = called & markers
    bare_atomic = any(_is_atomic_reference(d) for d in function.decorator_list)
    if bare_atomic and function.decorator_list:
        matched |= {_position(function.decorator_list[0])} & markers
    return matched
