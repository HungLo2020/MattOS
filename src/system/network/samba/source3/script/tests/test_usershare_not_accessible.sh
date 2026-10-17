#!/usr/bin/env bash
#
# Verify that smbd does not crash when a usershare definition file
# is removed while a client is connected.
#
# Regression test for BUG: https://bugzilla.samba.org/show_bug.cgi?id=14978
#
# The crash path was:
#   tree connect -> find_service() -> lp_servicenumber() ->
#   usershare_exists() fails -> free_service_byindex() destroys
#   service -> later request on same connection -> volume_label()
#   -> lp_servicename() -> falls back to sDefault.szService
#   (NULL) -> strlen(NULL) -> SIGSEGV
#

if [ $# -lt 5 ]; then
	cat <<EOF
Usage: test_usershare_not_accessible.sh SERVER SERVER_IP USERNAME PASSWORD SMBCLIENT <smbclient arguments>
EOF
	exit 1
fi

SERVER="$1"
SERVER_IP="$2"
USERNAME="$3"
PASSWORD="$4"
smbclient="$5"
shift 5
ADDARGS="$@"

failed=0

samba_bindir="$BINDIR"
samba_net="$samba_bindir/net"
samba_testparm="$samba_bindir/testparm"

samba_share_dir="$LOCAL_PATH"
samba_usershare_dir="$samba_share_dir/usershares"

# Get the usershare definition file path from smb.conf.
samba_usershare_path=$($samba_testparm $CONFIGURATION -s --parameter-name="usershare path" 2>/dev/null)
if [ -z "$samba_usershare_path" ] || [ ! -d "$samba_usershare_path" ]; then
	echo "Could not determine usershare path from smb.conf" >&2
	exit 1
fi

incdir=$(dirname $0)/../../../testprogs/blackbox
. $incdir/subunit.sh

samba_usershare_name="test_usershare_crash"
samba_usershare_sharedir="$samba_usershare_dir/$samba_usershare_name"

cleanup()
{
	if [ -f "$samba_usershare_path/$samba_usershare_name" ]; then
		chmod 644 "$samba_usershare_path/$samba_usershare_name" 2>/dev/null
	fi
	$samba_net usershare delete "$samba_usershare_name" 2>/dev/null
	rm -rf "$samba_usershare_sharedir" 2>/dev/null
	rm -f smbclient-stdin smbclient-stdout smbclient-stderr
}

# Clean up from any previous failed run.
cleanup

# Create the usershare directory and share.
mkdir --mode=0755 --verbose "$samba_usershare_sharedir"

$VALGRIND $samba_net usershare add \
	"$samba_usershare_name" \
	"$samba_usershare_sharedir" \
	"$samba_usershare_name"

cd $SELFTEST_TMPDIR || exit 1

# Set up named pipes for interactive smbclient session.
rm -f smbclient-stdin smbclient-stdout smbclient-stderr
mkfifo smbclient-stdin smbclient-stdout smbclient-stderr

CLI_FORCE_INTERACTIVE=1
export CLI_FORCE_INTERACTIVE

# Start a persistent smbclient connection to the usershare.
${smbclient} //${SERVER}/${samba_usershare_name} ${CONFIGURATION} \
	-U${USERNAME}%${PASSWORD} \
	<smbclient-stdin >smbclient-stdout 2>smbclient-stderr &
CLIENT_PID=$!

sleep 1

exec 100>smbclient-stdin 101<smbclient-stdout 102<smbclient-stderr

# Consume the smbclient startup message.
head -n 1 <&101

# Verify the connection works with an initial ls.
echo "ls" >&100
sleep 1
head -n 4 <&101

# Save the current tree ID so we can restore it after tcon
# replaces the client-side tcon object.
echo "tid" >&100
sleep 1
tid_output=$(head -n 2 <&101)
saved_tid=$(echo "$tid_output" | sed -n 's/current tid is //p')
if [ -z "$saved_tid" ]; then
	echo "ERROR: could not determine current tid from: $tid_output" >&2
	exit 1
fi

# Delete the usershare definition file while the connection
# is still active. This causes usershare_exists() to fail
# with ENOENT. The smb2 tree connect handler runs as root,
# so chmod is not sufficient — we must actually remove the
# file to trigger the failure.
rm -f "$samba_usershare_path/$samba_usershare_name"

# Verify the file is actually gone.
if [ -f "$samba_usershare_path/$samba_usershare_name" ]; then
	echo "ERROR: usershare definition file still exists at $samba_usershare_path/$samba_usershare_name" >&2
	exit 1
fi

# Issue a tree connect to the same usershare. This triggers
# find_service() -> lp_servicenumber() -> usershare_exists()
# which fails with ENOENT. Without the fix,
# lp_servicenumber() calls free_service_byindex() which
# destroys the service entry while the original tree connect
# is still active.
#
# The tcon will fail, but the critical side-effect is that
# lp_servicenumber() is called with the usershare name.
# It also replaces the client-side tcon object, so we must
# restore the original tree ID afterward.
echo "tcon ${samba_usershare_name}" >&100
sleep 1

# Restore the original tree ID. The server-side tree connect
# is still alive — the server doesn't know smbclient freed
# its client-side tcon object. Setting the tid back makes
# subsequent commands use the original server-side tree connect.
echo "tid ${saved_tid}" >&100
sleep 1

# Now issue 'volume' which triggers smbd_do_qfsinfo() ->
# volume_label() -> lp_servicename(). Without the fix, the
# service was destroyed by the tcon above and
# lp_servicename() returns NULL, causing strlen(NULL) ->
# SIGSEGV.
echo "volume" >&100
sleep 1

# If smbd crashed, this 'ls' will fail because the socket
# is dead. This verifies the smbd process survived.
echo "ls" >&100
echo "quit" >&100

sleep 1

# Close the write fd so smbclient can exit, then read
# whatever output is available. We can't use a fixed
# head -n count because if smbd crashed smbclient may
# have already exited with fewer lines than expected.
exec 100>&-
output=$(cat <&101)
exec 101<&- 102<&-
wait ${CLIENT_PID} 2>/dev/null

# Check that the ls output contains a directory listing
# (the "." entry), meaning smbd was alive after the
# usershare was deleted and the tcon triggered
# lp_servicenumber(). If smbd crashed, we'll see
# NT_STATUS_CONNECTION_DISCONNECTED instead.
subunit_start_test "smbd alive after usershare deleted during session"
if echo "$output" | grep -q 'NT_STATUS_CONNECTION_DISCONNECTED'; then
	echo "$output" | subunit_fail_test "smbd alive after usershare deleted during session"
	failed=$(expr $failed + 1)
elif echo "$output" | grep -q '^  \.'; then
	subunit_pass_test "smbd alive after usershare deleted during session"
else
	echo "$output" | subunit_fail_test "smbd alive after usershare deleted during session"
	failed=$(expr $failed + 1)
fi

# Clean up.
rm -rf "$samba_usershare_sharedir" 2>/dev/null
rm -f smbclient-stdin smbclient-stdout smbclient-stderr

testok $0 $failed
