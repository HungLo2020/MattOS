#!/bin/sh

# Copyright (C) 2015-2026 Free Software Foundation, Inc.
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

srcdir="${srcdir:-.}"

if ! test -z "${VALGRIND}";then
VALGRIND="${LIBTOOL:-libtool} --mode=execute ${VALGRIND} --error-exitcode=7"
fi

ASN1DECODING="${ASN1DECODING:-../src/asn1Decoding$EXEEXT}"
ASN1PKIX="${ASN1PKIX:-pkix.asn}"
TMPFILEOUTPUT=decoding.out.$$.tmp
FGREP=${FGREP:-fgrep}

$VALGRIND "$ASN1DECODING" "$ASN1PKIX" "${srcdir}"/TestCertOctetOverflow.der PKIX1.Certificate
if test $? != 1;then
	echo "Decoding failed"
	exit 1
fi

# test decoding of certificate with invalid time field
$VALGRIND "$ASN1DECODING" -s "$ASN1PKIX" "${srcdir}"/cert-invalid-time.der PKIX1.Certificate
if test $? != 1;then
	echo "Decoding with invalid time succeeded when not expected"
	exit 1
fi

# test decoding of certificate with invalid time field
$VALGRIND "$ASN1DECODING" -t "$ASN1PKIX" "${srcdir}"/cert-invalid-time.der PKIX1.Certificate
if test $? != 0;then
	echo "Decoding with invalid time failed when not expected"
	exit 1
fi

# test attempting to decode an invalid type
$VALGRIND "$ASN1DECODING" -t "$ASN1PKIX" "${srcdir}"/cert-invalid-time.der PKIX1.CErtificate
if test $? != 1;then
	echo "Decoding with an invalid type succeeded when not expected"
	exit 1
fi

# test attempting to decode a missing ASN file
$VALGRIND "$ASN1DECODING" -t missing.asn "${srcdir}"/cert-invalid-time.der PKIX1.Certificate
if test $? != 1;then
	echo "Decoding with a missing asn file succeeded when not expected"
	exit 1
fi

# Test invalid command line option
$VALGRIND "$ASN1DECODING" --asdf > $TMPFILEOUTPUT 2>&1
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

# Test invalid command line option "c"
$VALGRIND "$ASN1DECODING" -c > $TMPFILEOUTPUT 2>&1
if test $? != 1; then
	echo "Invalid command line arg (2) - incorrect return code!"
	exit 1
fi

# Test debug command line option "d" and an empty (0 byte) der file
touch "${srcdir}"/blank.der
$VALGRIND "$ASN1DECODING" -d "$ASN1PKIX" "${srcdir}"/blank.der PKIX1.Certificate
if test $? != 1; then
	echo "Blank der - incorrect return code!"
	exit 1
fi
rm "${srcdir}"/blank.der

# Test debug command line option "d" and an empty (0 byte) asn file
touch blank.asn
$VALGRIND "$ASN1DECODING" -d blank.asn "${srcdir}"/cert-invalid-time.der PKIX1.Certificate
if test $? != 1; then
	echo "Blank asn - incorrect return code!"
	exit 1
fi
rm blank.asn

# test benchmark option
$VALGRIND "$ASN1DECODING" -b "$ASN1PKIX" "${srcdir}"/cert-invalid-time.der PKIX1.Certificate > $TMPFILEOUTPUT 2>&1
if test $? != 0;then
	echo "Benchmark decoding - incorrect return code!"
	exit 1
fi

if ! $FGREP -q "structures/sec" $TMPFILEOUTPUT; then
	echo "Benchmark decoding - incorrect command output!"
    exit 1
fi

# test missing DER file
$VALGRIND "$ASN1DECODING" -b "$ASN1PKIX" "${srcdir}"/missing.der PKIX1.Certificate
if test $? != 1;then
	echo "Missing DER file - incorrect return code!"
	exit 1
fi

# Test help command line option
$VALGRIND "$ASN1DECODING" --help > $TMPFILEOUTPUT 2>&1
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
$VALGRIND "$ASN1DECODING" > $TMPFILEOUTPUT 2>&1
if test $? != 1; then
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
$VALGRIND "$ASN1DECODING" --version
if test $? != 0; then
	echo "Version command line arg - incorrect return code!"
	exit 1
fi

# Test passing an invalid filename
$VALGRIND "$ASN1DECODING" this_isnt_a_real_file.asn > $TMPFILEOUTPUT 2>&1
if test $? = 0; then
	echo "Test invalid filename - incorrect return code!"
	exit 1
fi

# Look for actual version in the output
if ! $FGREP -q "input files or ASN.1 type name missing" $TMPFILEOUTPUT; then
	echo "Test invalid filename - incorrect command output!"
    exit 1
fi

rm -f ${TMPFILEOUTPUT}

exit 0
