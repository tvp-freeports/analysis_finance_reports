"""Submodule containing all the utilities for validating and parsing the configuration"""

import os
from abc import ABC, abstractmethod
from enum import Enum, Flag
from typing import Optional, Annotated, Union
import argparse
import re
from pathlib import Path
import logging as log
from xdg import BaseDirectory
import yaml
from pydantic import (
    BaseModel,
    conint,
    PositiveInt,
    FilePath,
    HttpUrl,
    AfterValidator,
    BeforeValidator,
    model_validator,
    TypeAdapter,
)

from freeports_analysis.data import TARGET_LISTS
from freeports_analysis.formats.data import VALID_FORMATS, url_to_format
from freeports_analysis.i18n import _

from .consts import PROGRAM_DESCRIPTION, InputFlags, InputEnum

_logger = log.getLogger(__name__)


def _str_to_bool(string: str) -> bool:
    """Function used to convert a string consisting of a True/False value to a boolean
    Parameters
    ----------
    string : str
        The original string

    Returns
    -------
    bool
        The resulting Boolean

    Raises
    ------
    ValueError
        Raises error if the format is unrecognizable
    """
    true_list = ["true", "yes", "on", "t", "y", "1"]
    false_list = ["false", "no", "off", "f", "n", "0"]
    string = string.strip().lower()
    if string in true_list:
        return True
    if string in false_list:
        return False

    error_string = _("'{}' is not castable to `True` {} nor `False` {}").format(
        string, true_list, false_list
    )
    raise ValueError(error_string)


def _format_validate(format: str) -> str:
    """Functions that checks if a format is present in the formats list, returns it if it is,
    raises an error if it isnt'.
    Parameters
    ----------
    format : str
        The format name

    Returns
    -------
    str
        The format name if correct.

    Raises
    ------
    ValueError
        not a correct format, prints the complete lisr
    """
    if format not in VALID_FORMATS:
        raise ValueError(
            _("`{}` is not a valid format, valid formats are {}").format(
                format, VALID_FORMATS
            )
        )
    return format


Format = Annotated[str, AfterValidator(_format_validate)]
Lists = Annotated[list, BeforeValidator(lambda x: [x] if isinstance(x, str) else x)]
Verbosity = conint(ge=0, le=5)

_out_structure_both_modes = ["REGULAR", "SINGLE_FILE", "STRUCTURED"]
_out_structure_normal_mode = []
_out_structure_batch_mode = []
OutStructureNormalMode = Enum(
    "OutStructureNormalMode", _out_structure_both_modes + _out_structure_normal_mode
)
OutStructureBatchMode = Enum(
    "OutStructureBatchMode", _out_structure_both_modes + _out_structure_batch_mode
)

_out_flags_both_modes = ["COMPRESSED"]
_out_flags_normal_mode = []
_out_flags_batch_mode = ["SEPARATE_OUT_FILES"]
OutFlagsNormalMode = Flag(
    "OutFlagsNormalMode",
    _out_flags_both_modes + _out_flags_normal_mode,
)
OutFlagsBatchMode = Flag(
    "OutFlagsBatchMode",
    _out_flags_both_modes + _out_flags_batch_mode,
)

OutProfile = Union[InputEnum(OutStructureNormalMode), InputEnum(OutStructureBatchMode)]
OutFlags = Union[InputFlags(OutFlagsNormalMode), InputFlags(OutFlagsBatchMode)]


class SelectorOutProfile:
    @model_validator(mode="before")
    @classmethod
    def cast_to_right_type(cls, values):
        batch_file = values.get("BATCH_FILE")
        adapter_enum = TypeAdapter(InputEnum(OutStructureNormalMode))
        adapter_flags = TypeAdapter(InputFlags(OutFlagsNormalMode))
        if batch_file is not None:
            adapter_enum = TypeAdapter(InputEnum(OutStructureBatchMode))
            adapter_flags = TypeAdapter(InputFlags(OutFlagsBatchMode))
        out_profile = values.get("OUT_PROFILE")
        values["OUT_PROFILE"] = (
            adapter_enum.validate_python(out_profile)
            if out_profile is not None
            else None
        )
        out_flags = values.get("OUT_FLAGS")
        values["OUT_FLAGS"] = (
            adapter_flags.validate_python(out_flags) if out_flags is not None else None
        )
        return values


class ParitalConfiguration(ABC):
    @abstractmethod
    def model_dump(self, *args, **kargs):
        pass

    def overwrite_config(self, config, config_location):
        this_conf = self.model_dump()
        for k, v in this_conf.items():
            if v is not None:
                config[k] = v
                config_location[k] = self.__class__.__name__
        return config, config_location


class FreeportsFileConfig(BaseModel, SelectorOutProfile, ParitalConfiguration):
    VERBOSITY: Optional[Verbosity] = None
    OUT_PATH: Optional[Path] = None
    OUT_PROFILE: Optional[OutProfile] = None
    OUT_FLAGS: Optional[OutFlags] = None
    N_WORKERS: Optional[PositiveInt] = None
    BATCH_FILE: Optional[FilePath] = None
    SAVE_PDF: Optional[bool] = None
    URL: Optional[HttpUrl] = None
    PDF: Optional[Path] = None
    FORMAT: Optional[Format] = None
    TARGET_LISTS: Optional[Lists] = None

    @classmethod
    def _local_config(cls):
        # 1. Check local config file
        patterns = [
            r"^\.?(config|conf)[-\._]?freeports\.ya?ml$",
            r"^\.?freeports[-\._]?(config|conf)\.ya?ml$",
        ]

        for patter in patterns:
            for file_name in os.listdir("."):
                if not re.match(patter, file_name, re.IGNORECASE):
                    continue
                local_file = os.path.abspath(file_name)
                if not os.path.isfile(local_file):
                    continue
                return Path(local_file)
        return None

    @classmethod
    def _standard_config(cls):
        config_dirs = []
        # For Linux/Unix-like systems (including macOS)
        # 2. Check XDG config directories for 'freeports.yaml' directly
        if os.name == "posix":
            # XDG config directories
            config_dirs = BaseDirectory.load_config_paths("")

        # For Windows systems
        elif os.name == "nt":
            # Local AppData (user-specific config)
            local_appdata = os.environ.get("LOCALAPPDATA") or os.path.expanduser(
                "~\\AppData\\Local"
            )
            config_dirs.append(local_appdata)

            # ProgramData (system-wide config)
            program_data = os.environ.get("PROGRAMDATA") or "C:\\ProgramData"
            config_dirs.append(program_data)

        for config_dir in config_dirs:
            for file_name in ["freeports.yaml", "freeports.yml"]:
                config_path = os.path.join(config_dir, file_name)
                _logger.debug(
                    _("Searching `xdg`/`Windows` compliant conf file: '%s'"),
                    config_path,
                )
                if os.path.isfile(config_path):
                    return Path(config_path)
        return None

    @classmethod
    def _system_config(cls):
        system_paths = []
        if os.name == "posix":
            # 3. Fallback to /etc/freeports.yaml
            system_paths = ["/etc/freeports.yaml", "/etc/freeports.yml"]
        elif os.name == "nt":
            system_paths = [
                os.path.join(
                    os.environ.get("SystemRoot", "C:\\Windows"), "freeports.yaml"
                ),
                os.path.join(
                    os.environ.get("SystemRoot", "C:\\Windows"), "freeports.yml"
                ),
            ]

        for system_path in system_paths:
            _logger.debug("Searching system wise conf file: '%s'", system_path)
            if os.path.isfile(system_path):
                return Path(system_path)
        return None

    @classmethod
    def find_config(cls):
        config_file = cls._local_config()
        if config_file is not None:
            _logger.debug(_("Found local conf file: '%s'"), config_file)
            return config_file

        config_file = cls._standard_config()
        if config_file is not None:
            _logger.debug(
                _("Found `xdg`/`Windows` compliant conf file: '%s'"), config_file
            )
            return config_file

        config_file = cls._system_config()
        if config_file is not None:
            _logger.debug(_("Found system wise conf file: '%s'"), config_file)
            return config_file

        # 4. Not found
        _logger.debug(
            _(
                "Configuration not found in default location, `CONFIG_FILE` set to `None`"
            )
        )
        return None

    def __init__(self, config_file=None):
        _map_names = {
            "verbosity": "VERBOSITY",
            "separate_out": "SEPARATE_OUT_FILES",
            "out_path": "OPUT_PATH",
            "n_workers": "N_WORKERS",
            "batch_file": "BATCH_FILE",
            "save_pdf": "SAVE_PDF",
            "url": "URL",
            "format": "FORMAT",
            "target_lists": "TARGET_LISTS",
            "out_profile": "OUT_PROFILE",
            "out_flags": "OUT_FLAGS",
        }
        if config_file is None:
            config_file = self.find_config()
        if config_file is None:
            super().__init__()
            return
        config_file = Path(config_file)
        config_dict = yaml.safe_load(config_file.open("r", encoding="UTF-8"))
        config_dict = {_map_names[k]: v for k, v in config_dict.items()}
        super().__init__(**config_dict)


DEFAULT_CONFIG = {
    "PDF": None,
    "URL": None,
    "FORMAT": None,
    "CONFIG_FILE": FreeportsFileConfig.find_config(),
    "SAVE_PDF": True,
    "TARGET_LISTS": TARGET_LISTS,
    "VERBOSITY": 2,
    "N_WORKERS": os.process_cpu_count() if (os.name == "posix") else os.cpu_count(),
    "BATCH_FILE": None,
    "PREFIX_OUT": None,
    "OUT_PATH": Path("."),
    "OUT_PROFILE": OutStructureNormalMode.REGULAR,
    "OUT_FLAGS": OutFlagsNormalMode(0),
}
DEFAULT_CONFIG_LOCATION = {k: "FreeportsDefaultConfig" for k in DEFAULT_CONFIG}


class FreeportsEnvConfig(BaseModel, SelectorOutProfile, ParitalConfiguration):
    VERBOSITY: Optional[Verbosity] = None
    N_WORKERS: Optional[PositiveInt] = None
    BATCH_FILE: Optional[FilePath] = None
    OUT_PATH: Optional[FilePath] = None
    OUT_PROFILE: Optional[OutProfile] = None
    OUT_FLAGS: Optional[OutFlags] = None
    SAVE_PDF: Optional[bool] = None
    URL: Optional[HttpUrl] = None
    PDF: Optional[Path] = None
    FORMAT: Optional[Format] = None
    CONFIG_FILE: Optional[FilePath] = None
    TARGET_LISTS: Optional[Lists] = None

    def __init__(self):
        ENV_PREFIX = "FREEPORTS_"
        _map_names = {
            f"{ENV_PREFIX}URL": "URL",
            f"{ENV_PREFIX}VERBOSITY": "VERBOSITY",
            f"{ENV_PREFIX}N_WORKERS": "N_WORKERS",
            f"{ENV_PREFIX}BATCH_FILE": "BATCH_FILE",
            f"{ENV_PREFIX}OUT_PATH": "OUT_PATH",
            f"{ENV_PREFIX}OUT_PROFILE": "OUT_PROFILE",
            f"{ENV_PREFIX}OUT_FLAGS": "OUT_FLAGS",
            f"{ENV_PREFIX}SAVE_PDF": "SAVE_PDF",
            f"{ENV_PREFIX}FORMAT": "FORMAT",
            f"{ENV_PREFIX}PDF": "PDF",
            f"{ENV_PREFIX}CONFIG_FILE": "CONFIG_FILE",
            f"{ENV_PREFIX}TARGET_LIST": "TARGET_LISTS",
        }
        config_dict = {std_k: os.environ.get(k) for k, std_k in _map_names.items()}
        super().__init__(**config_dict)


class FreeportsCmdConfig(BaseModel, SelectorOutProfile, ParitalConfiguration):
    VERBOSITY: Optional[Verbosity] = None
    OUT_PROFILE: Optional[OutProfile] = None
    OUT_FLAGS: Optional[OutFlags] = None
    OUT_PATH: Optional[Path] = None
    N_WORKERS: Optional[PositiveInt] = None
    BATCH_FILE: Optional[FilePath] = None
    SAVE_PDF: Optional[bool] = None
    URL: Optional[HttpUrl] = None
    PDF: Optional[Path] = None
    FORMAT: Optional[Format] = None
    TARGET_LISTS: Optional[Lists] = None

    @classmethod
    def create_parser(self):
        parser = argparse.ArgumentParser(description=PROGRAM_DESCRIPTION)
        # Argomenti obbligatori (stringhe)
        parser.add_argument(
            "--url", "-u", type=str, help=_("URL of the dir where to find the pdf")
        )
        parser.add_argument("--pdf", "-i", type=str, help=_("Name of the file"))
        parser.add_argument(
            "--batch",
            "-b",
            type=str,
            help=_("Activate `BATCH MODE`, path of the batch file"),
        )
        help_str = _(
            "# parallel workers in `BATCH MODE`, if num <= 0, it set to # cpu avalaibles"
        )
        parser.add_argument("--workers", "-j", type=int, help=help_str)
        parser.add_argument("--format", "-f", type=str, help=_("PDF format"))
        parser.add_argument(
            "--no-download", action="store_true", help=_("Don't save file locally")
        )
        parser.add_argument(
            "--separate-out", action="store_true", help=_("Separate output files")
        )
        parser.add_argument(
            "--config", type=str, help=_("Custom configuration file location")
        )
        out_path = DEFAULT_CONFIG["OUT_PATH"]
        parser.add_argument(
            "--out",
            "-o",
            type=str,
            help=_("Output file cvs (default path: '{}')").format(out_path),
        )
        verb = DEFAULT_CONFIG["VERBOSITY"]
        parser.add_argument(
            "-v",
            action="count",
            help=_("Increase verbosity (default level: {})").format(verb),
        )
        parser.add_argument(
            "-q",
            action="count",
            help=_("Decrease verbosity (default level: {})").format(verb),
        )
        target_lists = DEFAULT_CONFIG["TARGET_LISTS"]
        parser.add_argument(
            "--target-list",
            "-T",
            type=str,
            help=_("List to filter the companies of interest (default: {})").format(
                target_lists
            ),
        )
        parser.add_argument(
            "--archive",
            "-z",
            action="store_true",
            help=_("Create a `.tar.gz` archive of the output"),
        )
        parser.add_argument(
            "--out-profile",
            "-P",
            type=str,
            help=_("Specify the structure of the output dataset"),
        )
        return parser

    def __init__(self, args, default_verbosity):
        args = vars(args)
        _map_names = {
            "url": "URL",
            "pdf": "PDF",
            "format": "FORMAT",
            "out": "OUT_PATH",
            "batch": "BATCH_FILE",
            "workers": "N_WORKERS",
            "out_profile": "OUT_PROFILE",
            "target_list": "TARGET_LISTS",
            "no_download": None,
            "v": None,
            "q": None,
            "separate_out": None,
            "archive": None,
        }
        config_dict = {
            k_std: args[k] for k, k_std in _map_names.items() if k_std is not None
        }
        increase_verbosity = 0
        if (args["v"] is not None) and (args["q"] is not None):
            raise argparse.ArgumentTypeError(
                _("Cannot increase and decrease verbosity!")
            )
        elif args["v"] is not None:
            increase_verbosity = args["v"]
        elif args["q"] is not None:
            increase_verbosity = args["q"]
        config_dict["VERBOSITY"] = (
            min(max(default_verbosity + increase_verbosity, 0), 5)
            if increase_verbosity != 0
            else None
        )
        config_dict["SAVE_PDF"] = False if args["no_download"] else None

        config_dict["OUT_FLAGS"] = None
        for k, v in {
            "separate_out": "SEPARATE_OUT_FILES",
            "archive": "COMPRESSED",
        }.items():
            if args[k]:
                if config_dict["OUT_FLAGS"] is None:
                    config_dict["OUT_FLAGS"] = v
                else:
                    config_dict["OUT_FLAGS"] += f" | {v}"
        super().__init__(**config_dict)


class FreeportsJobConfig(BaseModel, ParitalConfiguration):
    PREFIX_OUT: Optional[str] = None
    SAVE_PDF: bool = True
    URL: Optional[HttpUrl] = None
    PDF: Optional[Path] = None
    FORMAT: Format
    TARGET_LISTS: Optional[Lists] = None

    def __init__(self, row_dict):
        _map_names = {
            "url": "URL",
            "save pdf": "SAVE_PDF",
            "format": "FORMAT",
            "pdf": "PDF",
            "prefix out": "PREFIX_OUT",
            "target list": "TARGET_LISTS",
        }
        config_dict = {_map_names[k.strip().lower()]: v for k, v in row_dict.items()}
        super().__init__(**config_dict)


class FreeportsConfig(BaseModel):
    VERBOSITY: Verbosity
    N_WORKERS: PositiveInt
    BATCH_FILE: Optional[FilePath] = None
    SAVE_PDF: bool = True
    URL: Optional[HttpUrl] = None
    PDF: Optional[Path] = None
    FORMAT: Optional[Format] = None
    CONFIG_FILE: Optional[FilePath] = None
    TARGET_LISTS: Lists
    PREFIX_OUT: Optional[str] = None
    OUT_PROFILE: Union[OutStructureNormalMode, OutStructureBatchMode]
    OUT_FLAGS: Union[OutFlagsNormalMode, OutFlagsBatchMode]
    OUT_PATH: Path

    @model_validator(mode="after")
    def set_compress_flag(self):
        type_out_flags = type(self.OUT_FLAGS)
        if self.OUT_PATH.name.endswith(".tar.gz"):
            self.OUT_FLAGS = self.OUT_FLAGS | type_out_flags.COMPRESSED
            self.OUT_PATH = self.OUT_PATH.with_suffix("").with_suffix("")
        return self

    @model_validator(mode="after")
    def detect_format(self):
        if self.URL is not None:
            detected_format = url_to_format(self.URL)
            if self.FORMAT is None:
                self.FORMAT = detected_format
            elif self.FORMAT != detected_format:
                _logger.warning(
                    _("Selected format `%s` is different from detected one: %s"),
                    self.FORMAT,
                    detected_format,
                )
        if self.FORMAT is None:
            raise ValueError(_("Format has to be specified or detected..."))
        return self

    @model_validator(mode="after")
    def right_out_profile_type(self):
        if self.BATCH_FILE is not None:
            if not self.BATCH_FILE.exists():
                raise ValueError(
                    _("Insert valid batch file name not {}").format(self.BATCH_FILE)
                )
            if not (
                isinstance(self.OUT_PROFILE, OutStructureBatchMode)
                and isinstance(self.OUT_FLAGS, OutFlagsBatchMode)
            ):
                raise ValueError(_("Out profile and flags should be of the right type"))
        else:
            if not (
                isinstance(self.OUT_PROFILE, OutStructureNormalMode)
                and isinstance(self.OUT_FLAGS, OutFlagsNormalMode)
            ):
                raise ValueError(_("Out profile and flags should be of the right type"))
        return self

    @model_validator(mode="after")
    def out_path_exists(self):
        if not self.OUT_PATH.parent.exists():
            raise ValueError(
                _("Out path is not valid because directory '{}' doesn't exists").format(
                    self.OUT_PATH.parent
                )
            )
        return self

    @model_validator(mode="after")
    def out_path_single_file(self):
        if self.OUT_PROFILE == OutStructureNormalMode.SINGLE_FILE:
            if not self.OUT_PATH.name.endswith(".csv"):
                self.OUT_PATH = self.OUT_PATH / "out.csv"
        return self

    @model_validator(mode="after")
    def input_should_be_specified(self):
        if self.URL is None and self.PDF is None:
            string = _("You have to specify at least one input option: ")
            string += _("the url or the resource, the pdf file path or both")
            raise ValueError(string)
        return self

    @model_validator(mode="after")
    def pdf_path_validation(self):
        if self.PDF is None:
            return self
        if self.SAVE_PDF:
            if self.PDF.name.endswith(".pdf"):
                if not self.PDF.parent.exists():
                    raise ValueError(_("PDF path not valid"))
            else:
                if self.PDF.exists():
                    self.PDF = self.PDF / "report.pdf"
                elif self.PDF.parent.exists():
                    pass
                else:
                    raise ValueError(_("PDF path not valid"))
            return self
        if not self.PDF.exists():
            if self.URL is None:
                raise ValueError(_("Url don't specified and PDF not valid!!!"))
            else:
                _logger.warning("PDF is not valid, fallback to URL...")
                self.PDF = None
        return self


schema_job_csv_config = None


def log_config(logger: log.Logger, config: dict, config_location: dict):
    """Log with debug priority the configuration provided

    Parameters
    ----------
    logger : log.Logger
        the logger that has to log
    """
    locations = {"DEFAULT": [], "CONFIG_FILE": [], "ENV_VAR": [], "CMD_ARG": []}
    for k, v in config_location.items():
        if v == "FreeportsDefaultConfig":
            locations["DEFAULT"].append(k)
        elif v == "FreeportsFileConfig":
            locations["CONFIG_FILE"].append(k)
        elif v == "FreeportsEnvConfig":
            locations["ENV_VAR"].append(k)
        elif v == "FreeportsCmdConfig":
            locations["CMD_ARG"].append(k)
        else:
            raise ValueError(_("Unknown config location: {}").format(v))
    logger.debug(_("Resulting config: %s"), {k: v for k, v in config.items()})
    logger.debug(_("Resulting location: %s"), locations)
