"""What is fast enough to run at every commit here, and what is not.

`freeports_dev`'s suite is almost entirely in-process: it builds a dictionary, renders it, and reads
what came out. Two fixtures are not — `repo` and `database` call `init-format-repo` and
`setup-input-db`, and each of those bootstraps a whole repository by shelling out around twenty
times, to `freeports-validate` for the nine renderings of the grants report and to `freeports-dev`
for the five of the CI report. That is a little over two seconds in one fixture, against a commit
gate whose entire budget is five.

So the same rule `freeports_validate`'s suite already applies is applied here: **a test that starts
a process is `slow`, and slow is run deliberately.** The marking is derived from the fixtures a test
asks for rather than written on each test, so that it cannot go stale — a test added next year that
bootstraps a repository is marked by the act of asking for the fixture that bootstraps it, and one
that stops doing so stops being marked.

**Deferred is not dropped.** `make test-python-slow` runs these, records that it did, and
`freeports-dev ci-check` prints `python.slow` as `NOT RUN` until somebody has — on a production
branch, refusing the commit. What this project will not have is a suite that quietly never runs;
what it will have is one that runs when it is asked for and says so when it has not been.
"""

import pytest


#: Requesting any of these makes a test `slow`, because each one bootstraps a repository and every
#: bootstrap is twenty-odd process starts.
SLOW_FIXTURES = frozenset({"repo", "database"})


def pytest_collection_modifyitems(config, items):
    slow = pytest.mark.slow
    for item in items:
        if SLOW_FIXTURES & set(item.fixturenames):
            item.add_marker(slow)
