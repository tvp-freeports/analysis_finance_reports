"""Submodule containing all the utilities for validating and parsing the configuration"""

import os
from abc import ABC, abstractmethod
from enum import Enum, Flag
from typing import Optional, Annotated, Union, Any, Tuple, Dict, List
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

from .consts import PROGRAM_DESCRIPTION, input_flags, input_enum

_logger = log.getLogger(__name__)


def _str_to_bool(string: str) -> bool:
    """Convert a string representation of boolean values to actual boolean.

    Parameters
    ----------
    string : str
        String representation of boolean value. Accepts various common
        representations like 'true', 'false', 'yes', 'no', '1', '0', etc.

    Returns
    -------
    bool
        Boolean value corresponding to the input string

    Raises
    ------
    ValueError
        If the string cannot be recognized as a valid boolean representation

    Examples
    --------
    >>> _str_to_bool('true')
    True
    >>> _str_to_bool('no')
    False
    >>> _str_to_bool('1')
    True
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


def _format_validate(format_name: str) -> str:
    """Validate that a format name exists in the list of supported formats.

    Parameters
    ----------
    format_name : str
        Name of the format to validate

    Returns
    -------
    str
        The validated format name if it exists in supported formats

    Raises
    ------
    ValueError
        If the format is not found in the list of valid formats

    Notes
    -----
    This function is used by Pydantic validators to ensure only supported
    PDF processing formats are accepted in configuration.
    """
    if format_name not in VALID_FORMATS:
        raise ValueError(
            _("`{}` is not a valid format, valid formats are {}").format(
                format_name, VALID_FORMATS
            )
        )
    return format_name


Format = Annotated[str, AfterValidator(_format_validate)]
Lists = Annotated[
    List[str], BeforeValidator(lambda x: [x] if isinstance(x, str) else x)
]
Verbosity = conint(ge=0, le=5)

_out_structure_both_modes = ["REGULAR", "SINGLE_FILE", "STRUCTURED"]
_out_structure_normal_mode = []
_out_structurebatch_mode = []
OutStructureNormalMode = Enum(
    "OutStructureNormalMode", _out_structure_both_modes + _out_structure_normal_mode
)
OutStructureBatchMode = Enum(
    "OutStructureBatchMode", _out_structure_both_modes + _out_structurebatch_mode
)

_out_flags_both_modes = ["COMPRESSED"]
_out_flags_normal_mode = []
_out_flagsbatch_mode = ["SEPARATE_OUT_FILES"]
OutFlagsNormalMode = Flag(
    "OutFlagsNormalMode",
    _out_flags_both_modes + _out_flags_normal_mode,
)
OutFlagsBatchMode = Flag(
    "OutFlagsBatchMode",
    _out_flags_both_modes + _out_flagsbatch_mode,
)

OutProfile = Union[
    input_enum(OutStructureNormalMode), input_enum(OutStructureBatchMode)
]
OutFlags = Union[input_flags(OutFlagsNormalMode), input_flags(OutFlagsBatchMode)]


class SelectorOutProfile:
    """Mixin class for Pydantic models to handle output profile and flags type casting.

    This class provides validation logic to ensure output profiles and flags
    are cast to the appropriate type based on whether batch mode is active.

    Attributes
    ----------
    None
        This is a mixin class that adds validation behavior
    """

    @model_validator(mode="before")
    @classmethod
    def cast_to_right_type(cls, values: Dict[str, Any]) -> Dict[str, Any]:
        """Cast output profile and flags to the correct type based on processing mode.

        Parameters
        ----------
        values : Dict[str, Any]
            Dictionary of input values to validate and cast

        Returns
        -------
        Dict[str, Any]
            Validated dictionary with properly typed output profile and flags

        Notes
        -----
        This validator automatically detects whether batch mode is active
        (based on presence of BATCH_FILE) and casts OUT_PROFILE and OUT_FLAGS
        to the appropriate enum/flag types for that mode.
        """
        batch_file = values.get("BATCH_FILE")
        adapter_enum = TypeAdapter(input_enum(OutStructureNormalMode))
        adapter_flags = TypeAdapter(input_flags(OutFlagsNormalMode))
        if batch_file is not None:
            adapter_enum = TypeAdapter(input_enum(OutStructureBatchMode))
            adapter_flags = TypeAdapter(input_flags(OutFlagsBatchMode))
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
    """Abstract base class for partial configuration sources.

    This class represents a configuration source that provides partial
    configuration values, used to overwrite the main configuration dictionary
    and track the source of each configuration value.

    Attributes
    ----------
    None
        This is an abstract base class
    """

    @abstractmethod
    def model_dump(self, *args: Any, **kargs: Any) -> Dict[str, Any]:
        """Serialize the partial configuration to a dictionary.

        Returns
        -------
        Dict[str, Any]
            Dictionary containing the configuration values from this source

        Notes
        -----
        This method is typically implemented by Pydantic models that inherit
        from this class, providing automatic serialization of model fields.
        """

    def overwrite_config(
        self, config: Dict[str, Any], config_location: Dict[str, str]
    ) -> Tuple[Dict[str, Any], Dict[str, str]]:
        """Overwrite configuration with values from this partial configuration source.

        Parameters
        ----------
        config : Dict[str, Any]
            Current configuration dictionary to be updated
        config_location : Dict[str, str]
            Current configuration location tracking dictionary

        Returns
        -------
        Tuple[Dict[str, Any], Dict[str, str]]
            Tuple containing:
            - Updated configuration dictionary with overwritten values
            - Updated location dictionary tracking source of each value

        Notes
        -----
        Only non-None values from this configuration source will overwrite
        existing values in the configuration dictionary.
        """
        this_conf = self.model_dump()
        new_conf = dict(config.items())
        new_conf_location = dict(config_location.items())
        for k, v in this_conf.items():
            if v is not None:
                new_conf[k] = v
                new_conf_location[k] = self.__class__.__name__
        return new_conf, new_conf_location


class FreeportsFileConfig(BaseModel, SelectorOutProfile, ParitalConfiguration):
    """Represents the configuration portion loaded from a specific configuration file.

    This class handles the parsing and validation of configuration settings
    from YAML configuration files located in various standard locations.

    Attributes
    ----------
    VERBOSITY : Optional[Verbosity]
        The verbosity level for logging output
    OUT_PATH : Optional[Path]
        The output directory path for generated files
    OUT_PROFILE : Optional[OutProfile]
        The output structure profile (normal or batch mode)
    OUT_FLAGS : Optional[OutFlags]
        Additional output flags and options
    N_WORKERS : Optional[PositiveInt]
        Number of parallel workers for processing
    BATCH_FILE : Optional[FilePath]
        Path to batch file for batch processing mode
    SAVE_PDF : Optional[bool]
        Whether to save downloaded PDF files locally
    URL : Optional[HttpUrl]
        URL pointing to PDF resources
    PDF : Optional[Path]
        Local path to PDF file for processing
    FORMAT : Optional[Format]
        Format specification for PDF parsing
    TARGET_LISTS : Optional[Lists]
        Lists of target companies to filter during analysis
    """

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
    def _local_config(cls) -> Optional[Path]:
        """Search for configuration files in the current working directory.

        Returns
        -------
        Optional[Path]
            Path to local configuration file if found, None otherwise

        Notes
        -----
        Searches for files matching patterns like:
        - '.config-freeports.yaml'
        - 'config-freeports.yml'
        - 'freeports-config.yaml'
        - etc.
        """
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
    def _standard_config(cls) -> Optional[Path]:
        """Search for configuration in standard user configuration directories.

        Returns
        -------
        Optional[Path]
            Path to standard configuration file if found, None otherwise

        Notes
        -----
        On POSIX systems (Linux/macOS), searches XDG config directories.
        On Windows, searches Local AppData and ProgramData directories.
        Looks for 'freeports.yaml' or 'freeports.yml' files.
        """
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
    def _system_config(cls) -> Optional[Path]:
        """Search for configuration in system-wide standard locations.

        Returns
        -------
        Optional[Path]
            Path to system configuration file if found, None otherwise

        Notes
        -----
        On POSIX systems, searches /etc/freeports.yaml and /etc/freeports.yml.
        On Windows, searches Windows system directory.
        """
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
    def find_config(cls) -> Optional[Path]:
        """Find configuration file by searching in standard locations.

        Returns
        -------
        Optional[Path]
            Path to configuration file if found, None otherwise

        Notes
        -----
        Searches locations in the following order:
        1. Current working directory (various naming patterns)
        2. User configuration directories (XDG on POSIX, AppData on Windows)
        3. System-wide directories (/etc on POSIX, Windows system directory)

        Returns the first configuration file found in this search order.
        """
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

    def __init__(self, config_file: Optional[Path] = None):
        """Initialize FreeportsFileConfig by loading configuration from file.

        Parameters
        ----------
        config_file : Optional[Path], optional
            Path to configuration file, if None will search for default locations
        """
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
        config_dict = {}
        with config_file.open("r", encoding="UTF-8") as f:
            config_dict = yaml.safe_load(f)
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
    """Represents configuration loaded from environment variables.

    Attributes
    ----------
    VERBOSITY : Optional[Verbosity]
        The verbosity level for logging output
    N_WORKERS : Optional[PositiveInt]
        Number of parallel workers for processing
    BATCH_FILE : Optional[FilePath]
        Path to batch file for batch processing mode
    OUT_PATH : Optional[FilePath]
        The output directory path for generated files
    OUT_PROFILE : Optional[OutProfile]
        The output structure profile (normal or batch mode)
    OUT_FLAGS : Optional[OutFlags]
        Additional output flags and options
    SAVE_PDF : Optional[bool]
        Whether to save downloaded PDF files locally
    URL : Optional[HttpUrl]
        URL pointing to PDF resources
    PDF : Optional[Path]
        Local path to PDF file for processing
    FORMAT : Optional[Format]
        Format specification for PDF parsing
    CONFIG_FILE : Optional[FilePath]
        Path to custom configuration file
    TARGET_LISTS : Optional[Lists]
        Lists of target companies to filter during analysis
    """

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
        """Initialize FreeportsEnvConfig by loading configuration from environment variables."""
        env_prefix = "FREEPORTS_"
        _map_names = {
            f"{env_prefix}URL": "URL",
            f"{env_prefix}VERBOSITY": "VERBOSITY",
            f"{env_prefix}N_WORKERS": "N_WORKERS",
            f"{env_prefix}BATCH_FILE": "BATCH_FILE",
            f"{env_prefix}OUT_PATH": "OUT_PATH",
            f"{env_prefix}OUT_PROFILE": "OUT_PROFILE",
            f"{env_prefix}OUT_FLAGS": "OUT_FLAGS",
            f"{env_prefix}SAVE_PDF": "SAVE_PDF",
            f"{env_prefix}FORMAT": "FORMAT",
            f"{env_prefix}PDF": "PDF",
            f"{env_prefix}CONFIG_FILE": "CONFIG_FILE",
            f"{env_prefix}TARGET_LIST": "TARGET_LISTS",
        }
        config_dict = {std_k: os.environ.get(k) for k, std_k in _map_names.items()}
        super().__init__(**config_dict)


class FreeportsCmdConfig(BaseModel, ParitalConfiguration):
    """Represents configuration loaded from command line arguments.

    Attributes
    ----------
    VERBOSITY : Optional[Verbosity]
        The verbosity level for logging output
    OUT_PROFILE : Optional[OutProfile]
        The output structure profile (normal or batch mode)
    OUT_FLAGS : Optional[OutFlags]
        Additional output flags and options
    OUT_PATH : Optional[Path]
        The output directory path for generated files
    N_WORKERS : Optional[PositiveInt]
        Number of parallel workers for processing
    BATCH_FILE : Optional[FilePath]
        Path to batch file for batch processing mode
    SAVE_PDF : Optional[bool]
        Whether to save downloaded PDF files locally
    URL : Optional[HttpUrl]
        URL pointing to PDF resources
    PDF : Optional[Path]
        Local path to PDF file for processing
    FORMAT : Optional[Format]
        Format specification for PDF parsing
    TARGET_LISTS : Optional[Lists]
        Lists of target companies to filter during analysis
    """

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
    def create_parser(cls) -> argparse.ArgumentParser:
        """Create and configure the command line argument parser.

        Returns
        -------
        argparse.ArgumentParser
            Configured argument parser for command line interface
        """
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

    def __init__(self, args: argparse.Namespace, default_verbosity: int):
        """Initialize FreeportsCmdConfig by parsing command line arguments.

        Parameters
        ----------
        args : argparse.Namespace
            Parsed command line arguments
        default_verbosity : int
            Default verbosity level to use as baseline
        """
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
        if args["v"] is not None:
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


class FreeportsJobConfig(BaseModel, SelectorOutProfile, ParitalConfiguration):
    """Represents configuration for individual jobs in batch processing mode.

    Attributes
    ----------
    PREFIX_OUT : Optional[str]
        Prefix for output files
    SAVE_PDF : bool
        Whether to save downloaded PDF files locally
    URL : Optional[HttpUrl]
        URL pointing to PDF resources
    PDF : Optional[Path]
        Local path to PDF file for processing
    FORMAT : Format
        Format specification for PDF parsing
    TARGET_LISTS : Optional[Lists]
        Lists of target companies to filter during analysis
    """

    PREFIX_OUT: Optional[str] = None
    SAVE_PDF: bool = True
    URL: Optional[HttpUrl] = None
    PDF: Optional[Path] = None
    FORMAT: Format
    TARGET_LISTS: Optional[Lists] = None

    def __init__(self, row_dict: Dict[str, Any]):
        """Initialize FreeportsJobConfig from a row dictionary.

        Parameters
        ----------
        row_dict : Dict[str, Any]
            Dictionary containing job configuration data
        """
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


class FreeportsConfig(BaseModel, SelectorOutProfile):
    """Main configuration class that combines all configuration sources.

    This class represents the final validated configuration after merging
    defaults, file config, environment variables, and command line arguments.

    Attributes
    ----------
    VERBOSITY : Verbosity
        The verbosity level for logging output
    N_WORKERS : PositiveInt
        Number of parallel workers for processing
    BATCH_FILE : Optional[FilePath]
        Path to batch file for batch processing mode
    SAVE_PDF : bool
        Whether to save downloaded PDF files locally
    URL : Optional[HttpUrl]
        URL pointing to PDF resources
    PDF : Optional[Path]
        Local path to PDF file for processing
    FORMAT : Optional[Format]
        Format specification for PDF parsing
    CONFIG_FILE : Optional[FilePath]
        Path to custom configuration file
    TARGET_LISTS : Lists
        Lists of target companies to filter during analysis
    PREFIX_OUT : Optional[str]
        Prefix for output files
    OUT_PROFILE : Union[OutStructureNormalMode, OutStructureBatchMode]
        The output structure profile (normal or batch mode)
    OUT_FLAGS : Union[OutFlagsNormalMode, OutFlagsBatchMode]
        Additional output flags and options
    OUT_PATH : Path
        The output directory path for generated files
    """

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
    OUT_FLAGS: Union[OutFlagsNormalMode, OutFlagsBatchMode] = None
    OUT_PATH: Path

    @model_validator(mode="after")
    def set_compress_flag(self) -> "FreeportsConfig":
        """Set COMPRESSED flag if output path ends with .tar.gz.

        Returns
        -------
        FreeportsConfig
            The updated configuration instance
        """
        type_out_flags = type(self.OUT_FLAGS)
        if self.OUT_PATH.name.endswith(".tar.gz"):
            self.OUT_FLAGS = self.OUT_FLAGS | type_out_flags.COMPRESSED
            self.OUT_PATH = self.OUT_PATH.with_suffix("").with_suffix("")
        return self

    @model_validator(mode="after")
    def detect_format(self) -> "FreeportsConfig":
        """Detect format from URL if not explicitly specified.

        Returns
        -------
        FreeportsConfig
            The updated configuration instance

        Raises
        ------
        ValueError
            If format cannot be detected or specified
        """
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
    def right_out_profile_type(self) -> "FreeportsConfig":
        """Validate that output profile and flags match the processing mode.

        Returns
        -------
        FreeportsConfig
            The updated configuration instance

        Raises
        ------
        ValueError
            If output profile/flags don't match the processing mode
        """
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
    def out_path_exists(self) -> "FreeportsConfig":
        """Validate that the output path parent directory exists.

        Returns
        -------
        FreeportsConfig
            The updated configuration instance

        Raises
        ------
        ValueError
            If output path parent directory doesn't exist
        """
        if not self.OUT_PATH.parent.exists():
            raise ValueError(
                _("Out path is not valid because directory '{}' doesn't exists").format(
                    self.OUT_PATH.parent
                )
            )
        return self

    @model_validator(mode="after")
    def out_path_single_file(self) -> "FreeportsConfig":
        """Ensure output path has .csv extension for SINGLE_FILE mode.

        Returns
        -------
        FreeportsConfig
            The updated configuration instance
        """
        if self.OUT_PROFILE == OutStructureNormalMode.SINGLE_FILE:
            if not self.OUT_PATH.name.endswith(".csv"):
                self.OUT_PATH = self.OUT_PATH / "out.csv"
        return self

    @model_validator(mode="after")
    def input_should_be_specified(self) -> "FreeportsConfig":
        """Validate that at least one input source is specified.

        Returns
        -------
        FreeportsConfig
            The updated configuration instance

        Raises
        ------
        ValueError
            If neither URL nor PDF input is specified
        """
        if self.URL is None and self.PDF is None:
            string = _("You have to specify at least one input option: ")
            string += _("the url or the resource, the pdf file path or both")
            raise ValueError(string)
        return self

    @model_validator(mode="after")
    def pdf_path_validation(self) -> "FreeportsConfig":
        """Validate PDF path and handle SAVE_PDF logic.

        Returns
        -------
        FreeportsConfig
            The updated configuration instance

        Raises
        ------
        ValueError
            If PDF path is invalid and no URL is specified
        """
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
            _logger.warning("PDF is not valid, fallback to URL...")
            self.PDF = None
        return self
