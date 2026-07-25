#!/usr/bin/env python3
"""Create a NoCloud seed ISO for the Atom Arc Ubuntu VM."""

from __future__ import annotations

import argparse
from pathlib import Path

import pycdlib


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--user-data", required=True, type=Path)
    parser.add_argument("--meta-data", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    for source in (args.user_data, args.meta_data):
        if not source.is_file():
            raise FileNotFoundError(source)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists():
        args.output.unlink()

    image = pycdlib.PyCdlib()
    image.new(interchange_level=3, joliet=3, rock_ridge="1.09", vol_ident="CIDATA")
    image.add_file(
        str(args.user_data),
        iso_path="/USER_DAT.;1",
        joliet_path="/user-data",
        rr_name="user-data",
    )
    image.add_file(
        str(args.meta_data),
        iso_path="/META_DAT.;1",
        joliet_path="/meta-data",
        rr_name="meta-data",
    )
    image.write(str(args.output))
    image.close()


if __name__ == "__main__":
    main()
