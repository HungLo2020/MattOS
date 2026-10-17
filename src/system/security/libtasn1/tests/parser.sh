#!/bin/sh

# Copyright (C) 2021-2026 Free Software Foundation, Inc.
# Copyright (C) 2019 Red Hat, Inc.
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

PARSER="${PARSER:-../src/asn1Parser${EXEEXT}}"
srcdir="${srcdir:-.}"
FGREP=${FGREP:-fgrep}
TMPFILE=pkix.asn.$$.tmp
TMPFILEOUTPUT=parser.out.$$.tmp

if ! test -z "${VALGRIND}"; then
	VALGRIND="${LIBTOOL:-libtool} --mode=execute valgrind --leak-check=full"
fi

echo "Test: PKIX file generation"

${VALGRIND} "${PARSER}" "${srcdir}"/pkix.asn -o ${TMPFILE}

if test $? != 0;then
	echo "Cannot generate C file!"
	exit 1
fi

# Find out how to remove carriage returns from output. Solaris /usr/ucb/tr
# does not understand '\r'.
if echo solaris | tr -d '\r' | grep solais > /dev/null; then
  cr='\015'
else
  cr='\r'
fi
# normalize output
LC_ALL=C tr -d "$cr" < $TMPFILE > x$TMPFILE
mv x$TMPFILE $TMPFILE

cmp ${TMPFILE} ${srcdir}/pkix.asn.out || \
    diff ${TMPFILE} ${srcdir}/pkix.asn.out

if test $? != 0;then
    echo "Generated C file differs!"
    cat ${TMPFILE}
    exit 1
fi

rm -f ${TMPFILE}

# Test invalid command line option
${VALGRIND} "${PARSER}" --asdf > $TMPFILEOUTPUT 2>&1

if test $? != 1; then
	echo "Invalid command line arg - incorrect return code!"
	exit 1
fi

# Look for "--help" in the output, make grep quiet.
# "--" to avoid grep trying to interpret "--help" as an option.
if ! $FGREP -q -- "--help" $TMPFILEOUTPUT; then
	echo "Invalid command line arg - incorrect command output!"
    exit 1
fi

# Test help command line option
${VALGRIND} "${PARSER}" --help > $TMPFILEOUTPUT 2>&1

if test $? != 0; then
	echo "Help command line arg - incorrect return code!"
	exit 1
fi

# Look for "--help" in the output, make grep quiet.
# "--" to avoid grep trying to interpret "--help" as an option.
if ! $FGREP -q -- "--help" $TMPFILEOUTPUT; then
	echo "Help command line arg - incorrect command output!"
    exit 1
fi

# Test no options
${VALGRIND} "${PARSER}" > $TMPFILEOUTPUT 2>&1

if test $? != 0; then
	echo "No command line arg - incorrect return code!"
	exit 1
fi

# Look for "--help" in the output, make grep quiet.
# "--" to avoid grep trying to interpret "--help" as an option.
if ! $FGREP -q -- "--help" $TMPFILEOUTPUT; then
	echo "No command line arg - incorrect command output!"
    exit 1
fi

# Test version option
${VALGRIND} "${PARSER}" --version
if test $? != 0; then
	echo "Version command line arg - incorrect return code!"
	exit 1
fi

# Test check option - valid case
${VALGRIND} "${PARSER}" -c "${srcdir}"/Test_tree.asn > $TMPFILEOUTPUT 2>&1
if test $? != 0; then
	echo "Check command line arg (valid case) - incorrect return code!"
	exit 1
fi

# Look for actual version in the output
if $FGREP -q "Error:" $TMPFILEOUTPUT; then
	echo "Check command line arg (valid case) - incorrect command output!"
    exit 1
fi

# Test check option - invalid case
${VALGRIND} "${PARSER}" -c "${srcdir}"/Test_parser_ERROR.asn > $TMPFILEOUTPUT 2>&1
if test $? = 0; then
	echo "Check command line arg (invalid case)- incorrect return code!"
	exit 1
fi

# Test passing an invalid filename
${VALGRIND} "${PARSER}" this_isnt_a_real_file.asn > $TMPFILEOUTPUT 2>&1
if test $? = 0; then
	echo "Test invalid filename - incorrect return code!"
	exit 1
fi

# Another error case, causes "recursion" which falls to a default
# case in asn1Parser.c
${VALGRIND} "${PARSER}" -c "${srcdir}"/CVE-2018-1000654-2.asn > $TMPFILEOUTPUT 2>&1
if test $? = 0; then
	echo "Check recursion - incorrect return code!"
	exit 1
fi

rm -f ${TMPFILEOUTPUT}

exit 0
