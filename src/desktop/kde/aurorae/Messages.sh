#! /usr/bin/env bash
$EXTRACTRC `find . -name \*.kcfg -o -name \*.ui` >> rc.cpp || exit 11
$XGETTEXT rc.cpp `find . -name \*.cpp -o -name \*.qml` -o $podir/aurorae.pot
rm -f rc.cpp
