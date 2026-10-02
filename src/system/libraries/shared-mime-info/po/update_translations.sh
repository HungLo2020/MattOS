#!/bin/sh

set -e

if [ ! -f po/shared-mime-info.pot ] ; then
	echo "Please generate “po/shared-mime-info.pot” before running this command"
	exit 1
fi

if [ data/freedesktop.org.xml.in -nt po/shared-mime-info.pot ] ; then
	echo "Please update “po/shared-mime-info.pot” before running this command"
	exit 1
fi

if [ ! -f .tx/config ] ; then
	echo "Please run this command from the root of the source directory"
	exit 1
fi

# NOTE: tx should be installed from https://github.com/transifex/cli
# The older transifex-client project (in python) is outdated and rejected by the server

tx pull --all --force --skip
tx push --source
