import os
import sys
from pathlib import Path


SUBCOMMANDS = [
    "grant",
    "ungrant",
    "who-grants",
    "granted-by",
    "granted-with",
    "sign-document",
    "create-document",
    "check-grants",
    "update",
]


def main():
    pkg_dir = Path(__file__).parent
    bin_dir = pkg_dir / "bin"
    lib_dir = pkg_dir / "lib"
    docs_dir = pkg_dir / "docs"

    formats_repo = None

    args = list(sys.argv[1:])
    i = 0
    while i < len(args):
        if args[i] == "--repo" and i + 1 < len(args):
            formats_repo = args[i + 1]
            del args[i : i + 2]
            break
        i += 1

    if not formats_repo:
        formats_repo = os.environ.get("FREEPORTS_FORMATS_REPO", os.getcwd())

    if not args:
        print("Usage: freeports-validate <subcommand> [options]")
        print()
        print("Subcommands:")
        for sc in SUBCOMMANDS:
            print(f"  {sc}")
        print()
        print("Options:")
        print(
            "  --repo PATH   Path to formats repository (default: FREEPORTS_FORMATS_REPO or cwd)"
        )
        sys.exit(1)

    subcommand = args[0]
    script = bin_dir / subcommand

    if not script.exists():
        print(f"Unknown subcommand: {subcommand}")
        print(f"Available: {', '.join(SUBCOMMANDS)}")
        sys.exit(1)

    env = os.environ.copy()
    env["FREEPORTS_FORMATS_REPO"] = os.path.abspath(formats_repo)
    env["FREEPORTS_VALIDATE_LIB"] = str(lib_dir)
    env["FREEPORTS_VALIDATE_DOCS"] = str(docs_dir)

    os.execve(str(script), [str(script)] + args[1:], env)
