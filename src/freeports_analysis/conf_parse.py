import os
from xdg import BaseDirectory
from pathlib import Path
import logging as log
import yaml
from enum import Enum, auto
import re
from pathlib import Path
from .consts import ENV_PREFIX, PDF_Formats

_logger = log.getLogger(__name__)


def _find_config():
    # 1. Check local config file
    patterns = [
        r"^(config|conf)[-\._]?freeports\.ya?ml$",
        r"^freeports[-\._]?(config|conf)\.ya?ml$",
    ]

    for patter in patterns:
        for file_name in os.listdir("."):
            if re.match(patter, file_name, re.IGNORECASE):
                local_file = os.path.abspath(file_name)
                if os.path.isfile(local_file):
                    _logger.debug("Found local conf file: '%s'", local_file)
                    return Path(local_file)

    # 2. Check XDG config directories for 'freeports.yaml' directly
    for config_dir in BaseDirectory.load_config_paths(""):
        for file_name in ["freeports.yaml", "freeports.yml"]:
            config_path = os.path.join(config_dir, file_name)
            _logger.debug("Searching `xdg` compliant conf file: '%s'", config_path)
            if os.path.isfile(config_path):
                _logger.debug("Found `xdg` compliant conf file: '%s'", config_path)
                return Path(config_path)

    # 3. Fallback to /etc/freeports.yaml
    system_paths = ["/etc/freeports.yaml", "/etc/freeport.yaml"]
    for system_path in system_paths:
        _logger.debug("Searching system wise conf file: '%s'", system_path)
        if os.path.isfile(system_path):
            _logger.debug("Found system wise conf file: '%s'", system_path)
            return Path(system_path)

    # 4. Not found
    _logger.debug(
        "Configuration not found in default location, `CONFIG_FILE` set to `None`"
    )
    return None


DEFAULT_CONFIG = {
    "VERBOSITY": 2,
    "BATCH_WORKERS": None,
    "BATCH": None,
    "OUT_CSV": "/dev/stdout",
    "SAVE_PDF": True,  # default to `True` because command line args permits to set only to `False`
    "URL": None,
    "PDF": None,
    "FORMAT": None,
    "CONFIG_FILE": _find_config(),
}


schema_yaml_config = {
    "verbosity": ("VERBOSITY", int),
    "n_workers": ("BATCH_WORKERS", int),
    "pdf": ("PDF", Path),
    "batch_path": ("BATCH", Path),
    "out_path": ("OUT_CSV", Path),
    "save_pdf": ("SAVE_PDF", bool),
    "format": ("FORMAT", lambda x: PDF_Formats.__members__[x.strip()]),
}


def _str_to_bool(string: str) -> bool:
    true_list = ["true", "yes", "on", "t", "y", "1"]
    false_list = ["false", "no", "off", "f", "n", "0"]
    string = string.strip().lower()
    if string in true_list:
        return True
    elif string in false_list:
        return False
    else:
        raise ValueError(
            f"'{string}' is not castable to `True` {true_list} nor `False` {false_list}"
        )


schema_env_config = {
    f"{ENV_PREFIX}URL": ("URL", str),
    f"{ENV_PREFIX}VERBOSITY": ("VERBOSITY", int),
    f"{ENV_PREFIX}BATCH_WORKERS": ("BATCH_WORKERS", int),
    f"{ENV_PREFIX}BATCH": ("BATCH", Path),
    f"{ENV_PREFIX}OUT_CSV": ("OUT_CSV", Path),
    f"{ENV_PREFIX}SAVE_PDF": ("SAVE_PDF", _str_to_bool),
    f"{ENV_PREFIX}FORMAT": ("FORMAT", lambda x: PDF_Formats.__members__[x.strip()]),
    f"{ENV_PREFIX}PDF": ("PDF", Path),
    f"{ENV_PREFIX}CONFIG_FILE": ("CONFIG_FILE", Path),
}

schema_job_csv_config = {
    "url": ("URL", str),
    "save pdf": ("SAVE_PDF", _str_to_bool),
    "format": ("FORMAT", lambda x: PDF_Formats.__members__[x.strip()]),
    "pdf": ("PDF", Path),
    "prefix out": ("PREFIX_OUT_CSV", str),
}


class POSSIBLE_LOCATION_CONFIG(Enum):
    DEFAULT = auto()
    CONFIG_FILE = auto()
    ENV_VAR = auto()
    CMD_ARG = auto()
    JOB_OVERWRITE = auto()


RESULTING_LOCATION_CONFIG = {
    k: POSSIBLE_LOCATION_CONFIG.DEFAULT for k in DEFAULT_CONFIG
}
RESULTING_CONFIG = {k: v for k, v in DEFAULT_CONFIG.items()}


def log_resultig_config(logger):
    logger.debug(
        "Resulting config: %s",
        {
            k: (RESULTING_CONFIG[k], RESULTING_LOCATION_CONFIG[k].name)
            for k in RESULTING_CONFIG
        },
    )


def overwrite_with_config_file(config, config_location):
    yaml_from_file = None
    config_file = config["CONFIG_FILE"]
    if config_file is not None:
        with config["CONFIG_FILE"].open("r", encoding="UTF-8") as f:
            yaml_from_file = yaml.safe_load(f)
        for key, v in yaml_from_file.items():
            conf_name, cast = schema_yaml_config[key]
            config[conf_name] = cast(v)
            config_location[conf_name] = POSSIBLE_LOCATION_CONFIG.CONFIG_FILE
    return config, config_location


def overwrite_with_env_vars(config, config_location):
    for key, (opt_name, cast) in schema_env_config.items():
        v = os.environ.get(key)
        if v is not None:
            config[opt_name] = cast(v)
            config_location[opt_name] = POSSIBLE_LOCATION_CONFIG.ENV_VAR
    return config, config_location


def apply_config(config: dict, config_location: dict):
    config, config_location = overwrite_with_config_file(config, config_location)
    config, config_location = overwrite_with_env_vars(config, config_location)
    return config, config_location


def get_config_file(config: dict, config_location: dict):
    env_conf_file = os.environ.get(f"{ENV_PREFIX}CONFIG_FILE")
    if env_conf_file is not None:
        config["CONFIG_FILE"] = env_conf_file
        config_location["CONFIG_FILE"] = POSSIBLE_LOCATION_CONFIG.ENV_VAR
    return config, config_location


def validate_conf(config: dict):
    if config["VERBOSITY"] > 5 or config["VERBOSITY"] < 0:
        raise ValueError(
            "Verbosity must be between 0 and 5, resulting is {}".format(
                config["VERBOSITY"]
            )
        )
    if config["BATCH_WORKERS"] is not None and config["BATCH_WORKERS"] <= 0:
        raise ValueError(
            "Cannot specify a number of workers < 1, resulting {}".format(
                config["BATCH_WORKERS"]
            )
        )
    batch_path = config["BATCH"]
    out_path = config["OUT_CSV"]
    if config["URL"] is None and config["PDF"] is None:
        raise ValueError(
            "You have to specify or the pdf url or the pdf file path or both"
        )
    if not out_path.parent.exists():
        raise ValueError(
            "Out path is not valid because {} doesn't exists".format(out_path.parent)
        )
    if batch_path is not None:
        if not batch_path.exists() or not batch_path.is_file():
            raise ValueError(f"Batch has to be existent file [{batch_path}]")
        if "." in out_path.name and not out_path.name.endswith(".tar.gz"):
            raise ValueError(
                f"Out file in `BATCH MODE` should be directory or `.tar.gz` file, resulting '{out_path}'"
            )
