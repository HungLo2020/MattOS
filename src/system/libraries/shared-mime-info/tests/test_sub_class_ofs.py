#!/usr/bin/env python3

import sys
import xml.etree.ElementTree as ET

FDO_XMLNS = "{http://www.freedesktop.org/standards/shared-mime-info}"


def test_sub_class_ofs(fdo_xml: str) -> int:

    with open(fdo_xml, "rb") as f:
        tree = ET.parse(f)

    errors = 0

    mime_types = set()
    sub_class_ofs = set()
    for elem in tree.findall(f"{FDO_XMLNS}mime-type"):
        mime_type = elem.attrib["type"]
        mime_types.add(mime_type)
        for elem in elem.findall(f"{FDO_XMLNS}sub-class-of"):
            sub_class_of = elem.attrib["type"]

            if mime_type == sub_class_of:
                print(f"{fdo_xml}: type {mime_type} is sub-class-of itself",
                      file=sys.stderr)
                errors += 1

            sub_class_ofs.add(sub_class_of)

    assert mime_types and sub_class_ofs

    for not_found in sub_class_ofs - mime_types:
        print(f"{fdo_xml}: parent sub-class-of type {not_found} not found",
              file=sys.stderr)
        errors += 1

    return errors


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {__file__} FREEDESKTOP_ORG_XML", file=sys.stderr)
        sys.exit(2)

    sys.exit(test_sub_class_ofs(sys.argv[1]) and 1 or 0)
