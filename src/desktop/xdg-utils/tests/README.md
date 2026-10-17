# xdg-utils tests

Created 6/27/2006 by Tom Whipple <tom.whipple@intel.com>

## Running xdg-utils Tests

At a minimum, you must have your current directory be the same as the
directory containing this README.

Then execute:
```
$ ./testrun -R
WARNING: guessed XDG_TEST_DIR to be /home/tw/portland/xdg-utils/tests
TEST_LIST:  generic_bogus_arg-1-1 ... generic_version-1-50
...
FAIL: test_user_mime_install
NORESULT: test_system_mime_install
71 of 92 tests passed. (114 attempted)
See xdg-test.log for details.
NOT OK!

FAIL     indicates (not surprisingly) a test failure.
NORESULT indicates that the test prerequisites failed for some reason. 
         (e.g. the install phase of an uninstall test failed)
UNTESTED means that something needed was not found. This is fine and should
         be ignored. These tests are not counted in the total, only attempted. 
         (e.g. test requires root, but we are not running as root)

NOTE: The test runner makes guesses about appropriate values of XDG_TEST_DIR
      and PATH. These values can be overridden explicitly.
```

<b>Note:</b> The `-R` flag disables tests that require root privileges,
remove it to include those. See [Privileged Tests](#privileged-tests)

To run tests individually, or as smaller groups do something like

```sh
./testrun -R xdg-mime
```

OR

```sh
./testrun -R xdg-mime/t.10-user_mime_install
```

OR (if you have defined `XDG_TEST_DIR` and `PATH` correctly)

```sh
xdg-mime/t.10-user_mime_install
```

## Create Backups!

**These tests change your user environment.**

Effort is made to keep pollution to a minimum, but we make no guarantees!!

Back up your environment/system early and often.
This is especially critical if you run tests as root.
You have been warned. 


## Interactive Tests

Because it is difficult to verify the way things appear to the user, some
tests are interactive and require the user to verify or perform actions.

This is sometimes annoying, so interactive tests can be disabled with the
`-I` flag. Note that if you run tests non-interactively, some tests
(xdg-email) may generate strange errors on the screen, since the test
cleans up support files before the email client tries to read them. Use
the `-C` option to work around this.


## Privileged Tests

[**BACK UP YOUR SYSTEM.** See above.](#create-backups)

Some tests require root (e.g. those commands with a `--system` option). So,
tests in this group return `UNTESTED` if they are not run as root. 

The test runner will ask for the root password in order to run these tests
as the root user.

To disable privileged tests, use the `-R` option.

## Cleanup

Tests should clean up after themselves. However, this sometimes fails,
so use `sudo make tests-clean` or `make tests-userclean`.

(Note that you must have generated a makefile via `cd .. && ./configure`
at some point.) 

 
## Directory Structure

* `xdg-* ` - tests for each util
* [include](include) -	"library" code used by most tests
* [generic](generic) - generic tests to be run on most utilities.
  i.e. [xdg-mime/t.00-apply_generic](xdg-mime/t.00-apply_generic)


## Writing xdg-utils Tests

See [xdg-mime/t.10-user_mime_install](xdg-mime/t.10-user_mime_install) as an example.

Each test is as follows

```sh
# Tests are functions for TET integration.
test_function() {

	# required to begin a test
	test_start "test description"  

	# optionally provide a verbose description. (not used)
	test_purpose "verbose text"	   

	# optionally begin a prerequisite section.
	# assertions that fail here cause NORESULT
	# rather than FAIL
	test_init

	# pre-assertions go here

	# required to begin the actual test assertions
	test_procedure

	# test-assertions go here
	
	# required to generate result codes.
	# Must be last
	test_result
}

run_test test_function
#  - OR -
repeat_test test_function NVARS V1 ... VN V1val1 ... V1valM ... VNval1 ... VNvalM
```

One of `run_test` or `repeat_test` is required,
see [include/testcontrol.sh](include/testcontrol.sh) for detail.

## Feedback

For questions or feedback, please use the Portland mailinglist at
http://lists.freedesktop.org/mailman/listinfo/portland

Test results and issues can be filed on the
[xdg-utils issue tracker on the freedesktop.org gitlab](https://gitlab.freedesktop.org/xdg/xdg-utils/-/issues).
