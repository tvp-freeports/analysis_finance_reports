"""``docs.python``: what fraction of the public Python objects here carry a docstring.

**Not sphinx's number, and the difference is the whole point of this module.**
``sphinx.ext.coverage`` measures whether an object *appears in the built site*, which is a fact
about how ``autosummary`` is configured rather than about whether anything is documented. Reading
its source showed two things that make it unusable as a threshold:

* every module of the compiled ``freeports`` extension reports **100.00 %**, because
  ``inspect.getmembers`` attributes nothing to it — a vacuous 100, not a perfect one;
* its TOTAL line is a union over object names, which is why it reads 25.93 % while nearly every row
  above it reads 100 %.

A threshold on that would be a threshold on the documentation build's configuration, and the first
time somebody changed an ``autosummary`` template the gate would move for no reason anybody could
explain. So ``make docs-site-coverage`` keeps reporting on the site — a real question, worth
asking — and the *gated* figure is measured here: an AST walk counting public modules, classes,
functions and methods, and how many of them have a docstring.

That is the exact counterpart of what ``rustdoc --show-coverage`` counts on the other side of this
repository, which is what makes ``docs.rust`` and ``docs.python`` two numbers a person can compare.

No dependency, and no import of the code being measured: it parses. Measuring by importing would
run module-level code, need every dependency of every module installed, and fail on the half of a
formats repository that expects an input database to exist.

*Alternative evaluated:* ``interrogate``, which does this with ``--fail-under`` and JSON output. It
was rejected only because a formats repository installs ``freeports-dev`` and nothing else, so it
would become a dependency of every format author's environment for a metric they may never turn on.
If it is ever wanted, this file is the one thing to swap.
"""

import ast
from pathlib import Path

from freeports_dev.ci import report


#: Directories that are never somebody's source: build output, caches, environments, vendored code.
_SKIP_DIRECTORIES = {
    ".git",
    ".venv",
    "venv",
    "build",
    "dist",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".benchmarks",
    "node_modules",
    "target",
}


def is_public(name):
    """Whether a name is part of what this package offers, by Python's own convention.

    A leading underscore means private and is not counted, in either direction: an undocumented
    helper does not lower the figure, and documenting one does not raise it. ``__init__`` and its
    siblings are the exception — they are public behaviour written in a private-looking spelling,
    and a class whose constructor is undocumented is a class whose constructor is undocumented.
    """
    if name.startswith("__") and name.endswith("__"):
        return True
    return not name.startswith("_")


class DocumentableObject:
    """One module, class, function or method, and whether it says what it is for."""

    def __init__(self, kind, qualified_name, path, line, documented):
        self.kind = kind
        self.qualified_name = qualified_name
        self.path = path
        self.line = line
        self.documented = documented


class _Walker(ast.NodeVisitor):
    """Collects the public documentable objects of one parsed module.

    Nested functions are not counted. A closure inside a function is an implementation detail of
    that function, and counting it would let a well-documented module be dragged down by the way
    somebody chose to factor one of its bodies — which is a fact about style, not about
    documentation.
    """

    def __init__(self, module_name, path):
        self.module_name = module_name
        self.path = path
        self.found = []
        self._scope = []

    def _record(self, node, kind):
        name = node.name
        qualified = ".".join([self.module_name, *self._scope, name])
        self.found.append(
            DocumentableObject(
                kind,
                qualified,
                self.path,
                node.lineno,
                ast.get_docstring(node) is not None,
            )
        )

    def visit_ClassDef(self, node):
        if not is_public(node.name):
            return
        self._record(node, "class")
        self._scope.append(node.name)
        for child in node.body:
            if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                if is_public(child.name):
                    self._record(child, "method")
            elif isinstance(child, ast.ClassDef):
                self.visit_ClassDef(child)
        self._scope.pop()

    def visit_FunctionDef(self, node):
        if self._scope:
            return
        if is_public(node.name):
            self._record(node, "function")

    visit_AsyncFunctionDef = visit_FunctionDef


def walk_module(path, module_name=None):
    """Every public documentable object of one file, the module itself included.

    A file that does not parse yields nothing and says so through the exception, which the caller
    turns into a finding: a syntax error is the linter's news to break, not this command's, and a
    documentation figure that silently drops a file would be a figure about a different codebase.
    """
    path = Path(path)
    source = path.read_text(encoding="utf-8")
    tree = ast.parse(source, filename=str(path))
    module_name = module_name or path.stem
    walker = _Walker(module_name, path)
    found = [
        DocumentableObject(
            "module", module_name, path, 1, ast.get_docstring(tree) is not None
        )
    ]
    for node in tree.body:
        walker.visit(node)
    return found + walker.found


def module_name_for(path, root):
    """A dotted module name for a file, relative to the root it was found under.

    ``__init__.py`` takes its package's name rather than becoming ``package.__init__``, so that the
    per-module breakdown reads the way an import statement does.
    """
    relative = Path(path).relative_to(root).with_suffix("")
    parts = list(relative.parts)
    if parts and parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts) if parts else Path(root).name


def python_files(root):
    """Every ``.py`` file under ``root`` that is somebody's source rather than build output."""
    root = Path(root)
    found = []
    for path in sorted(root.rglob("*.py")):
        if any(part in _SKIP_DIRECTORIES for part in path.relative_to(root).parts):
            continue
        found.append(path)
    return found


class DocstringCoverage:
    """The objects found under one root, and the ratio read off them."""

    def __init__(self, objects=None, problems=None):
        self.objects = list(objects or [])
        self.problems = list(problems or [])

    @property
    def total(self):
        return len(self.objects)

    @property
    def documented(self):
        return sum(1 for obj in self.objects if obj.documented)

    @property
    def ratio(self):
        """A root holding no public object is fully documented, not undocumented.

        There is nothing in it anybody failed to write about. Reading that as 0 % would refuse a
        commit to a package that has nothing wrong with it.
        """
        return 100.0 if not self.total else 100.0 * self.documented / self.total

    @property
    def undocumented(self):
        """The objects holding the figure down, qualified name and line, in file order."""
        return [obj for obj in self.objects if not obj.documented]

    def by_module(self):
        """The per-module breakdown, as ``{module: percent}``."""
        totals = {}
        for obj in self.objects:
            module = obj.qualified_name.split(".")[0] if obj.kind == "module" else None
            module = (
                obj.qualified_name
                if obj.kind == "module"
                else obj.qualified_name.rsplit(".", 1)[0]
            )
            counts = totals.setdefault(module, [0, 0])
            counts[1] += 1
            if obj.documented:
                counts[0] += 1
        return {
            module: 100.0 * documented / total
            for module, (documented, total) in sorted(totals.items())
        }


def measure(root, metric="docs.python", head=None):
    """Walk a directory of Python and read the docstring figure off it."""
    root = Path(root)
    objects = []
    problems = []
    for path in python_files(root):
        try:
            objects.extend(walk_module(path, module_name_for(path, root)))
        except (SyntaxError, UnicodeDecodeError, OSError) as exc:
            problems.append(f"{path}: {exc}")
    coverage = DocstringCoverage(objects, problems)
    return coverage, report.Measurement(
        metric,
        value=coverage.ratio,
        unit="percent",
        head=head,
        breakdown=coverage.by_module(),
        detail={
            "documented": coverage.documented,
            "total": coverage.total,
            "undocumented": [
                f"{obj.qualified_name} ({obj.kind}, {obj.path}:{obj.line})"
                for obj in coverage.undocumented
            ],
            "unreadable": coverage.problems,
        },
    )
