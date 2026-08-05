import os
import shutil
from pathlib import Path

import freeports_lib
from freeports._internals.input.companies_db import get_target_companies
from freeports._internals.cli.conf_parse import FreeportsFileConfig


def get_default_input_db_path():
    return Path(__file__).parent / "data" / "input_db"


def resolve_input_db(rootdir=None):
    env_path = os.environ.get("FREEPORTS_INPUT_DB_PATH")
    if env_path:
        p = Path(env_path)
        if p.exists():
            return p

    if rootdir is not None:
        local_db = Path(rootdir) / "tests" / "input_db"
        if local_db.exists():
            return local_db

    try:
        cfg_path = FreeportsFileConfig.find_config()
        if cfg_path:
            cfg = FreeportsFileConfig(cfg_path)
            if cfg.INPUT_DB_PATH:
                p = Path(cfg.INPUT_DB_PATH)
                if p.exists():
                    return p
    except Exception:
        pass

    default = get_default_input_db_path()
    if default.exists():
        return default

    raise FileNotFoundError(
        "No input DB found. Set FREEPORTS_INPUT_DB_PATH env var "
        "or create tests/input_db/ in the format repository."
    )


def get_test_companies(rootdir=None):
    input_db = resolve_input_db(rootdir)
    df = get_target_companies(input_db, ["TEST"])
    return freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(
        df
    )


def copy_default_input_db(target):
    src = get_default_input_db_path()
    if not src.exists():
        raise FileNotFoundError(f"Default input DB not found at {src}")
    if (target / "input_db").exists():
        return
    shutil.copytree(src, target / "input_db")
