#!/bin/sh

. "${TEST_SCRIPTS_DIR}/unit.sh"

define_test "Add/release 1 IP, interface is altname"

setup

ctdb_get_1_public_address |
	while read -r dev ip bits; do
		altname="alt${dev}"
		ip link property add dev "$dev" altname "$altname"

		ok_null
		simple_test_event "takeip" "$altname" "$ip" "$bits"

		ok_null
		simple_test_event "releaseip" "$altname" "$ip" "$bits"
	done
