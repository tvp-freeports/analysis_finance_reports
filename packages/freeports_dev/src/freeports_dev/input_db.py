"""Finding the input database a format repository's tests should search.

The order here is **not** the plain command-line-then-environment-then-file order of
:mod:`freeports_dev.config`, and the difference is deliberate. A format repository that ships its own
``tests/input_db`` must use *that* one, ahead of whatever real database the machine has configured,
or its tests would quietly start searching for different companies and produce different output. So
the repository's own database beats the configuration file, and only something named outright — the
``--db-directory`` flag or ``FREEPORTS_INPUT_DB_PATH`` — beats the repository.
"""

import shutil
from pathlib import Path

from freeports.input import get_target_companies

from freeports_dev.config import active


def get_default_input_db_path():
    return Path(__file__).parent / "data" / "input_db"


def resolve_input_db(rootdir=None, config=None):
    """The database to search, by the order this module's docstring describes.

    ``config`` is an already-built :class:`~freeports_dev.config.DevConfig` when the caller has one;
    the pytest plugin has none to build it from, so it falls back to whatever the process resolved.
    """
    config = config if config is not None else active()

    override = config.input_db_override
    if override:
        p = Path(override).expanduser()
        if p.exists():
            return p

    if rootdir is not None:
        local_db = Path(rootdir) / "tests" / "input_db"
        if local_db.exists():
            return local_db

    from_file = config.input_db_from_file
    if from_file:
        p = Path(from_file).expanduser()
        if p.exists():
            return p

    default = get_default_input_db_path()
    if default.exists():
        return default

    raise FileNotFoundError(
        "No input DB found. Pass --db-directory, set FREEPORTS_INPUT_DB_PATH, give `db_path` in the "
        "configuration file, or create tests/input_db/ in the format repository."
    )


def get_test_companies(rootdir=None, target_lists=None, config=None):
    # `get_target_companies` (Phase D, packages/freeports_engine/src/input/companies_db.rs) now
    # returns an already-compiled `List[CompanyMatchInfos]` directly, not a `pd.DataFrame` for
    # this function to compile itself — see the module doc on `companies_db.py`.
    config = config if config is not None else active()
    input_db = resolve_input_db(rootdir, config)
    if target_lists is None:
        target_lists = config.target_lists
    return get_target_companies(input_db, target_lists)


def copy_default_input_db(target):
    src = get_default_input_db_path()
    if not src.exists():
        raise FileNotFoundError(f"Default input DB not found at {src}")
    if (target / "input_db").exists():
        return
    shutil.copytree(src, target / "input_db")


def copy_default_input_db_into(target):
    """Copy the packaged example database's files *into* `target`, which is the database itself.

    `copy_default_input_db` puts the example one level down, as `input_db/`, because its caller owns
    a formats repository and wants a database inside it. `init-input-db` builds the database
    directory itself, so the example has to land in that directory rather than beside it — and it
    lands on top of the header-only files already written there, which is what makes the seeded and
    the empty database the same shape.
    """
    src = get_default_input_db_path()
    if not src.exists():
        raise FileNotFoundError(f"Default input DB not found at {src}")
    shutil.copytree(src, target, dirs_exist_ok=True)
