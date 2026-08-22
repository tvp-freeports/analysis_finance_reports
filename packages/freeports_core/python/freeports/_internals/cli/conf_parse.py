"""Submodule containing all the utilities for validating and parsing the configuration"""

import os
from abc import ABC, abstractmethod
from dataclasses import dataclass
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
    DirectoryPath,
    HttpUrl,
    BeforeValidator,
    model_validator,
    TypeAdapter,
    field_validator,
)

from freeports._internals.formats.repo.metadata import url_to_format, get_formats
from freeports.i18n import _

from freeports._internals.commons.consts import PROGRAM_DESCRIPTION
from freeports._internals.commons.enum_utils import input_flags, input_enum


DOC_SPEC_SEPARATOR = "|"
_URL_RE = re.compile(r'(https?://[^:"]+(?:"[^"]*")?)')


class DocumentSpec(BaseModel):
    """A single document specification with optional url, path, and name.

    At least one of ``url`` or ``path`` must be set. The ``name`` is used as
    the ``Report`` column in output; if None it falls back to url or path.
    """

    url: Optional[HttpUrl] = None
    path: Optional[Path] = None
    name: Optional[str] = None

    @classmethod
    def from_str(cls, specifier: str):
        specifier = specifier.strip()
        url_schema = None
        first_escaped = False
        if len(specifier) == 0:
            return cls(url=None, path=None, name=None)
        else:
            if specifier[0] == '"':
                first_escaped = True
                specifier = specifier[1:]
        for schema in ("http://", "https://"):
            if specifier.startswith(schema):
                url_schema = schema
                specifier = specifier.removeprefix(schema)
                break
        if first_escaped:
            specifier = '"' + specifier
        segments = []
        quote_area = False
        begin_segment = True
        for t in specifier:
            if t == '"' and not quote_area and begin_segment:
                quote_area = True
            elif t == '"' and quote_area:
                quote_area = False
            else:
                if begin_segment:
                    segments.append("")
                    begin_segment = False
                if t == ":" and not quote_area:
                    begin_segment = True
                else:
                    segments[-1] += t

        if len(segments) == 1:
            s0 = segments[0]
            if url_schema is None:
                path = Path(os.path.abspath(s0))
                return cls(url=None, path=path, name=str(path))
            else:
                url = url_schema + s0
                return cls(url=url, path=None, name=url)

        if len(segments) == 2:
            if begin_segment:
                url = url_schema + segments[0]
                path = Path(segments[1]).resolve()
                return cls(url=url, path=path, name=url)
            else:
                name = segments[1]
                if url_schema is None:
                    path = Path(os.path.abspath(segments[0]))
                    return cls(url=None, path=path, name=name)
                else:
                    url = url_schema + segments[0]
                    return cls(url=url, path=None, name=name)

        if len(segments) == 3:
            url = url_schema + segments[0]
            path = Path(os.path.abspath(segments[1]))
            name = segments[2]
            return cls(url=url, path=path, name=name)
        else:
            raise ValueError(
                f"Document specification parsing error, segment extracted are {segments} input was {specifier}"
            )

    @model_validator(mode="after")
    def input_should_be_specified(self) -> "DocumentSpec":
        """Validate that at least one input source is specified.

        Returns
        -------
        DocumentSpec
            The validated configuration instance

        Raises
        ------
        ValueError
            If neither URL nor PDF input is specified
        """
        if self.name is None:
            if self.url is not None:
                self.name = str(self.url)
            elif self.path is not None:
                self.name = str(self.path)
        if self.url is None and self.path is None:
            string = _("You have to specify at least one input option: ")
            string += _("the url or the resource, the pdf file path or both")
            raise ValueError(string)
        return self

    # @model_validator(mode="after")
    # def pdf_path_validation(self) -> "DocumentSpec":
    #     """Validate document specs, resolve directories, handle save_pdf logic.

    #     Produces a resolved list of ``DocumentSpec`` stored on the instance
    #     as ``_document_specs`` (not serialized via model_dump).
    #     """
    #     raw_specs: List[str] = []

    #     if self.DOCUMENT_SPECS:
    #         raw_specs = self.DOCUMENT_SPECS
    #     elif self.PDF is not None or self.URL is not None:
    #         url_str = str(self.URL) if self.URL else None
    #         pdf_str = str(self.PDF) if self.PDF else None
    #         if url_str and pdf_str:
    #             raw_specs = [f"{url_str}:{pdf_str}"]
    #         elif url_str:
    #             raw_specs = [url_str]
    #         elif pdf_str:
    #             raw_specs = [pdf_str]
    #     else:
    #         return self

    #     docs: List[DocumentSpec] = []
    #     for raw in raw_specs:
    #         try:
    #             ds = _parse_single_spec(raw)
    #         except ValueError:
    #             _logger.warning("Skipping invalid document spec: %s", raw)
    #             continue

    #         ds = _do_validate_document_spec(ds, self.SAVE_PDF)
    #         if ds is not None:
    #             docs.append(ds)

    #     if not docs:
    #         if self.URL is None:
    #             raise ValueError(
    #                 _("No valid documents found and no URL to fall back to")
    #             )
    #         _logger.warning("No valid local documents, will use URL...")

    #     object.__setattr__(self, "_document_specs", docs)
    #     return self


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


Format = str
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
    FORMATS_REPO_PATH: Optional[DirectoryPath] = None
    INPUT_DB_PATH: Optional[DirectoryPath] = None

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
            "out_path": "OUT_PATH",
            "n_workers": "N_WORKERS",
            "batch_file": "BATCH_FILE",
            "save_pdf": "SAVE_PDF",
            "url": "URL",
            "format": "FORMAT",
            "target_lists": "TARGET_LISTS",
            "out_profile": "OUT_PROFILE",
            "out_flags": "OUT_FLAGS",
            "formats_repo": "FORMATS_REPO_PATH",
            "db_path": "INPUT_DB_PATH",
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
    "DOCUMENT_SPECS": None,
    "FORMAT": None,
    "CONFIG_FILE": FreeportsFileConfig.find_config(),
    "SAVE_PDF": True,
    "TARGET_LISTS": None,
    "VERBOSITY": 2,
    "N_WORKERS": os.process_cpu_count() if (os.name == "posix") else os.cpu_count(),
    "BATCH_FILE": None,
    "PREFIX_OUT": None,
    "OUT_PATH": Path("."),
    "OUT_PROFILE": OutStructureNormalMode.REGULAR,
    "OUT_FLAGS": OutFlagsNormalMode(0),
    "FORMATS_REPO_PATH": None,
    "INPUT_DB_PATH": None,
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
    FORMATS_REPO_PATH: Optional[DirectoryPath] = None
    INPUT_DB_PATH: Optional[DirectoryPath] = None

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
            f"{env_prefix}FORMATS_REPO_PATH": "FORMATS_REPO_PATH",
            f"{env_prefix}INPUT_DB_PATH": "INPUT_DB_PATH",
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
    INPUT_REPORT : Optional[str]
        Input document specifier
    FORMAT : Optional[Format]
        Format specification for PDF parsing
    TARGET_LISTS : Optional[Lists]
        Lists of target companies to filter during analysis
    """

    VERBOSITY: Optional[Verbosity] = None
    INPUT_REPORTS: Optional[List[DocumentSpec]] = None
    OUT_PROFILE: Optional[OutProfile] = None
    OUT_FLAGS: Optional[OutFlags] = None
    OUT_PATH: Optional[Path] = None
    N_WORKERS: Optional[PositiveInt] = None
    BATCH_FILE: Optional[FilePath] = None
    SAVE_PDF: Optional[bool] = None
    FORMAT: Optional[Format] = None
    TARGET_LISTS: Optional[Lists] = None
    FORMATS_REPO_PATH: Optional[DirectoryPath] = None
    INPUT_DB_PATH: Optional[DirectoryPath] = None

    @classmethod
    def create_parser(cls) -> argparse.ArgumentParser:
        """Create and configure the command line argument parser.

        Returns
        -------
        argparse.ArgumentParser
            Configured argument parser for command line interface
        """
        parser = argparse.ArgumentParser(description=PROGRAM_DESCRIPTION)
        parser.add_argument(
            "--input",
            "--report",
            "-i",
            nargs="+",
            type=str,
            help=_("PDF file(s), directory, URL(s) specifier"),
        )
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
            nargs="+",
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
        parser.add_argument(
            "--db-directory",
            "-I",
            type=str,
            help=_("Specify the location of the input database"),
        )
        parser.add_argument(
            "--formats-directory",
            "-F",
            "--repo",
            "-r",
            type=str,
            help=_("Specify the location of the package containing formats"),
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
            "input": "INPUT_REPORTS",
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
            "formats_directory": "FORMATS_REPO_PATH",
            "db_directory": "INPUT_DB_PATH",
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
        if config_dict["INPUT_REPORTS"] is not None:
            config_dict["INPUT_REPORTS"] = [
                DocumentSpec.from_str(d) for d in config_dict["INPUT_REPORTS"]
            ]
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
            "report": "PREFIX_OUT",
            "prefix out": "PREFIX_OUT",
            "target list": "TARGET_LISTS",
        }
        config_dict = {_map_names[k.strip().lower()]: v for k, v in row_dict.items()}
        if "PDF" in config_dict and DOC_SPEC_SEPARATOR in str(config_dict["PDF"]):
            config_dict["DOCUMENT_SPECS"] = config_dict.pop("PDF").split(
                DOC_SPEC_SEPARATOR
            )
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
    INPUT_REPORTS: List[DocumentSpec]
    FORMAT: Optional[Format] = None
    CONFIG_FILE: Optional[FilePath] = None
    TARGET_LISTS: Lists
    OUT_PROFILE: Union[OutStructureNormalMode, OutStructureBatchMode]
    OUT_FLAGS: Union[OutFlagsNormalMode, OutFlagsBatchMode] = None
    OUT_PATH: Path
    INPUT_DB_PATH: DirectoryPath = None
    FORMATS_REPO_PATH: DirectoryPath = None

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
        detected_format = None
        tmp_detected_format = None
        if self.FORMATS_REPO_PATH is not None:
            for d in self.INPUT_REPORTS:
                if d.url is not None:
                    formats_df = get_formats(self.FORMATS_REPO_PATH)
                    format_names = formats_df.index.to_list()
                    tmp_detected_format = url_to_format(
                        self.FORMATS_REPO_PATH, format_names, d.url
                    )
                    if tmp_detected_format is not None:
                        if detect_format is None:
                            detect_format = tmp_detected_format
                        elif tmp_detected_format != detected_format:
                            raise ValueError(
                                f"Detected format different input reports, previous detected was {detected_format}, this is {detected_format}"
                            )
        if detected_format is not None:
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
    def validate_document_specs(self) -> "FreeportsConfig":
        """Validate and expand a single ``DocumentSpec``.

        Rules
        -----
        - **No URL, no save_pdf effect**: path must be valid dir or existing PDF.
        - **Directory**: will be scanned via ``rglob("*.pdf")`` later.
        Exception: ``save_pdf=False`` + URL → warn, fallback to URL (return None).
        - **File**: if exists → OK.  If missing + URL: ``save_pdf=True`` checks parent
        dir; ``save_pdf=False`` warns and falls back.  If missing + no URL → error.
        """

        new_ds = []
        for d in self.INPUT_REPORTS:
            if d.url is None:
                if d.path is None:
                    raise ValueError(f"The specified path {d.path} is not specified")
                elif d.path.is_dir():
                    for r in d.path.glob("*.pdf"):
                        new_ds.append(
                            DocumentSpec(
                                url=None, path=r, name=str(Path(d.name) / r.name)
                            )
                        )
                elif d.path.is_file():
                    new_ds.append(d)
                else:
                    raise ValueError(f"The specified path {d.path} does not exist")

            else:
                if d.path is None:
                    if self.SAVE_PDF:
                        _logger.warning(
                            "Specified SAVE_PDF but no path was selected, so the option is ignored"
                        )
                        self.SAVE_PDF = False
                    new_ds.append(d)
                elif d.is_dir():
                    if self.SAVE_PDF:
                        d.path = d.path / "report.pdf"
                        new_ds.append(d)
                    else:
                        for r in d.path.glob("*.pdf"):
                            new_ds.append(DocumentSpec(url=d.url, path=r, name=str(r)))
                elif d.parent.exist():
                    if d.exist():
                        self.SAVE_PDF = False
                        new_ds.append(d)
                    else:
                        if self.SAVE_PDF:
                            new_ds.append(d)
                        else:
                            _logger.warning(
                                "Invalid file '%s' specified with save_pdf=False and URL present, "
                                "falling back to URL",
                                d.path,
                            )
                            new_ds.append(d)
                else:
                    _logger.warning(
                        "Invalid file '%s' specified with save_pdf=False and URL present, "
                        "falling back to URL",
                        d.path,
                    )
                    new_ds.append(d)
        self.INPUT_REPORTS = new_ds
        return self
