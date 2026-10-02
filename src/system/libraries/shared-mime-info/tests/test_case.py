#!/usr/bin/env python3

from collections import defaultdict
import itertools
import sys
import xml.etree.ElementTree as ET

FDO_XMLNS = "{http://www.freedesktop.org/standards/shared-mime-info}"


def test_case(fdo_xml: str) -> int:

    with open(fdo_xml, "rb") as f:
        tree = ET.parse(f)

    errors = 0

    types = defaultdict(set)
    for elem in itertools.chain(
        tree.findall(f"{FDO_XMLNS}mime-type"),
        tree.findall(f"{FDO_XMLNS}mime-type/{FDO_XMLNS}alias"),
        tree.findall(f"{FDO_XMLNS}mime-type/{FDO_XMLNS}sub-class-of"),
    ):
        type_ = elem.attrib["type"]
        types[type_.lower()].add(type_)
    assert types

    for type_, spellings in types.items():
        if len(spellings) != 1:
            print(
                f"{fdo_xml}: multiple spellings differing only by case "
                f"for {', '.join(sorted(spellings))}",
                file=sys.stderr,
            )
            errors += 1

    return errors


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {__file__} FREEDESKTOP_ORG_XML", file=sys.stderr)
        sys.exit(2)

    sys.exit(test_case(sys.argv[1]) and 1 or 0)
