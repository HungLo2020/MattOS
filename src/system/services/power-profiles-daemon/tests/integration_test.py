#!/usr/bin/python3

# power-profiles-daemon integration test suite
#
# Run in built tree to test local built binaries, or from anywhere else to test
# system installed binaries.
#
# Copyright: (C) 2011 Martin Pitt <martin.pitt@ubuntu.com>
# (C) 2020 Bastien Nocera <hadess@hadess.net>
# (C) 2021 David Redondo <kde@david-redondo.de>
#
# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation; either version 2 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.

import os
import subprocess
import signal
import sys
import tempfile
import time
import unittest

import dbus

try:
    import gi
    from gi.repository import GLib
    from gi.repository import Gio
except ImportError as e:
    sys.stderr.write(
        f"Skipping tests, PyGobject not available for Python 3, or missing GI typelibs: {str(e)}\n"
    )
    sys.exit(77)

try:
    gi.require_version("UMockdev", "1.0")
    from gi.repository import UMockdev
except ImportError:
    sys.stderr.write("Skipping tests, umockdev not available.\n")
    sys.stderr.write("(https://github.com/martinpitt/umockdev)\n")
    sys.exit(77)

try:
    import dbusmock
except ImportError:
    sys.stderr.write("Skipping tests, python-dbusmock not available.\n")
    sys.stderr.write("(http://pypi.python.org/pypi/python-dbusmock)")
    sys.exit(77)


# pylint: disable=too-many-public-methods,too-many-instance-attributes
class Tests(dbusmock.DBusTestCase):
    """Dbus based integration unit tests"""

    PP = "org.freedesktop.UPower.PowerProfiles"
    PP_PATH = "/org/freedesktop/UPower/PowerProfiles"
    PP_INTERFACE = "org.freedesktop.UPower.PowerProfiles"

    @classmethod
    def setUpClass(cls):
        # run from local build tree if we are in one, otherwise use system instance
        builddir = os.getenv("top_builddir", ".")
        if os.access(os.path.join(builddir, "src", "power-profiles-daemon"), os.X_OK):
            cls.daemon_path = os.path.join(builddir, "src", "power-profiles-daemon")
            print(f"Testing binaries from local build tree {cls.daemon_path}")
        elif os.environ.get("UNDER_JHBUILD", False):
            jhbuild_prefix = os.environ["JHBUILD_PREFIX"]
            cls.daemon_path = os.path.join(
                jhbuild_prefix, "libexec", "power-profiles-daemon"
            )
            print(f"Testing binaries from JHBuild {cls.daemon_path}")
        else:
            cls.daemon_path = None
            with open(
                "/usr/lib/systemd/system/power-profiles-daemon.service",
                encoding="utf-8",
            ) as tmpf:
                for line in tmpf:
                    if line.startswith("ExecStart="):
                        cls.daemon_path = line.split("=", 1)[1].strip()
                        break
            assert (
                cls.daemon_path
            ), "could not determine daemon path from systemd .service file"
            print(f"Testing installed system binary {cls.daemon_path}")

        # fail on CRITICALs on client and server side
        GLib.log_set_always_fatal(
            GLib.LogLevelFlags.LEVEL_WARNING
            | GLib.LogLevelFlags.LEVEL_ERROR
            | GLib.LogLevelFlags.LEVEL_CRITICAL
        )
        os.environ["G_DEBUG"] = "fatal_warnings"

        # set up a fake system D-BUS
        cls.start_system_bus()
        cls.dbus = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)

    def start_dbus_template(self, template, parameters):
        process, dbus_object = self.spawn_server_template(
            template, parameters, stdout=subprocess.PIPE
        )

        def stop_template():
            process.stdout.close()
            try:
                process.kill()
            except OSError:
                pass
            process.wait()

        self.addCleanup(stop_template)
        self.assertTrue(process)
        self.assertTrue(dbus_object)

        return process, dbus_object, stop_template

    def setUp(self):
        """Set up a local umockdev testbed.

        The testbed is initially empty.
        """
        self.testbed = UMockdev.Testbed.new()

        def del_testbed():
            del self.testbed

        self.addCleanup(del_testbed)
        self.proxy = None
        self.props_proxy = None
        self.log = None
        self.daemon = None
        self.changed_properties = {}

        # Used for dytc devices
        self.tp_acpi = None

        self.polkitd, self.obj_polkit, _ = self.start_dbus_template("polkitd", {})
        self.obj_polkit.SetAllowed(
            [
                "org.freedesktop.UPower.PowerProfiles.switch-profile",
                "org.freedesktop.UPower.PowerProfiles.hold-profile",
                "org.freedesktop.UPower.PowerProfiles.configure-action",
                "org.freedesktop.UPower.PowerProfiles.configure-battery-aware",
            ]
        )

    def run(self, result=None):
        super().run(result)
        if not result or not self.log:
            return
        if len(result.errors) + len(result.failures) or os.getenv("PPD_TEST_VERBOSE"):
            with open(self.log.name, encoding="utf-8") as tmpf:
                sys.stderr.write("\n-------------- daemon log: ----------------\n")
                sys.stderr.write(tmpf.read())
                sys.stderr.write("------------------------------\n")

    #
    # Daemon control and D-BUS I/O
    #

    def start_daemon(self, args=None):
        """Start daemon and create DBus proxy.

        When done, this sets self.proxy as the Gio.DBusProxy for power-profiles-daemon.
        """
        env = os.environ.copy()
        env["G_DEBUG"] = "fatal-criticals"
        env["G_MESSAGES_DEBUG"] = "all"
        # note: Python doesn't propagate the setenv from Testbed.new(), so we
        # have to do that ourselves
        env["UMOCKDEV_DIR"] = self.testbed.get_root_dir()
        env["LD_PRELOAD"] = os.getenv("PPD_LD_PRELOAD") + " " + os.getenv("LD_PRELOAD")
        self.log = tempfile.NamedTemporaryFile()  # pylint: disable=consider-using-with
        daemon_path = [self.daemon_path, "-vv"]
        if args:
            daemon_path += args
        if os.getenv("PPD_TEST_WRAPPER"):
            daemon_path = os.getenv("PPD_TEST_WRAPPER").split(" ") + daemon_path
        elif os.getenv("VALGRIND"):
            daemon_path = ["valgrind"] + daemon_path

        # pylint: disable=consider-using-with
        self.daemon = subprocess.Popen(
            daemon_path, env=env, stdout=self.log, stderr=sys.stderr
        )
        self.addCleanup(self.stop_daemon, delete_profile=True)

        def on_proxy_connected(_, res):
            try:
                self.proxy = Gio.DBusProxy.new_finish(res)
                print(f"Proxy to {self.proxy.get_name()} connected")
            except GLib.Error as exc:
                self.fail(exc)

        cancellable = Gio.Cancellable()
        self.addCleanup(cancellable.cancel)
        Gio.DBusProxy.new(
            self.dbus,
            Gio.DBusProxyFlags.DO_NOT_AUTO_START,
            None,
            self.PP,
            self.PP_PATH,
            self.PP_INTERFACE,
            cancellable,
            on_proxy_connected,
        )

        # wait until the daemon gets online
        wait_time = 20 if "valgrind" in daemon_path[0] else 5
        self.assert_eventually(
            lambda: self.proxy and self.proxy.get_name_owner(),
            timeout=wait_time * 1000,
            message=lambda: f"daemon did not start in {wait_time} seconds: "
            + f"proxy is {self.proxy} and owner "
            + f"{self.proxy.get_name_owner() if self.proxy else 'None'}",
        )

        def properties_changed_cb(_, changed_properties, invalidated):
            self.changed_properties.update(changed_properties.unpack())

        self.addCleanup(
            self.proxy.disconnect,
            self.proxy.connect("g-properties-changed", properties_changed_cb),
        )

        self.assertEqual(self.daemon.poll(), None, "daemon crashed")

    def ensure_dbus_properties_proxies(self):
        self.props_proxy = Gio.DBusProxy.new_sync(
            self.dbus,
            Gio.DBusProxyFlags.DO_NOT_AUTO_START
            | Gio.DBusProxyFlags.DO_NOT_AUTO_START_AT_CONSTRUCTION
            | Gio.DBusProxyFlags.DO_NOT_LOAD_PROPERTIES
            | Gio.DBusProxyFlags.DO_NOT_CONNECT_SIGNALS,
            None,
            self.PP,
            self.PP_PATH,
            "org.freedesktop.DBus.Properties",
            None,
        )

    def stop_daemon(self, delete_profile=False):
        """Stop the daemon if it is running."""

        if self.daemon:
            try:
                self.daemon.terminate()
            except OSError:
                pass
            self.assertEqual(self.daemon.wait(timeout=3000), 0)

        if delete_profile:
            try:
                os.remove(self.testbed.get_root_dir() + "/" + "ppd_test_conf.ini")
            except (AttributeError, FileNotFoundError):
                pass

        self.daemon = None
        self.proxy = None

    def get_dbus_property(self, name):
        """Get property value from daemon D-Bus interface."""
        self.ensure_dbus_properties_proxies()
        return self.props_proxy.Get("(ss)", self.PP, name)

    def set_dbus_property(self, name, value):
        """Set property value on daemon D-Bus interface."""
        self.ensure_dbus_properties_proxies()
        return self.props_proxy.Set("(ssv)", self.PP, name, value)

    def call_dbus_method(self, name, parameters):
        """Call a method of the daemon D-Bus interface."""
        return self.proxy.call_sync(
            name, parameters, Gio.DBusCallFlags.NO_AUTO_START, -1, None
        )

    def have_text_in_log(self, text):
        return self.count_text_in_log(text) > 0

    def count_text_in_log(self, text):
        with open(self.log.name, encoding="utf-8") as tmpf:
            return tmpf.read().count(text)

    def read_file_contents(self, path):
        """Get the contents of a file"""
        with open(path, "rb") as tmpf:
            return tmpf.read()

    def read_sysfs_file(self, path):
        return self.read_file_contents(
            self.testbed.get_root_dir() + "/" + path
        ).rstrip()

    def read_sysfs_attr(self, device, attribute):
        return self.read_sysfs_file(device + "/" + attribute)

    def get_mtime(self, device, attribute):
        return os.path.getmtime(
            self.testbed.get_root_dir() + "/" + device + "/" + attribute
        )

    def write_file_contents(self, path, contents):
        """Set the contents of a file"""
        with open(path, "wb") as tmpf:
            return tmpf.write(
                contents if isinstance(contents, bytes) else contents.encode("utf-8")
            )

    def write_sysfs_file(self, path, contents):
        """Writes a sysfs file"""
        return self.write_file_contents(
            self.testbed.get_root_dir() + "/" + path, contents
        )

    def write_sysfs_attr(self, device, attribute, contents):
        """Writes a sysfs attribute"""
        return self.write_sysfs_file(device + "/" + attribute, contents)

    def change_immutable(self, fname, enable):
        attr = "-"
        if enable:
            os.chmod(fname, 0o444)
            self.addCleanup(self.change_immutable, fname, False)
            attr = "+"
        if os.geteuid() == 0:
            if not GLib.find_program_in_path("chattr"):
                self.skipTest("chattr is not found")

            try:
                subprocess.check_output(["chattr", f"{attr}i", fname])
            except subprocess.CalledProcessError as error:
                self.skipTest(f"chattr is not supported: {error.output}")
        if not enable:
            os.chmod(fname, 0o666)

    def create_dytc_device(self):
        self.tp_acpi = self.testbed.add_device(
            "platform",
            "thinkpad_acpi",
            None,
            ["dytc_lapmode", "0\n"],
            ["DEVPATH", "/devices/platform/thinkpad_acpi"],
        )
        self.addCleanup(self.testbed.remove_device, self.tp_acpi)

    def create_amd_apu(self):
        proc_dir = os.path.join(self.testbed.get_root_dir(), "proc/")
        os.makedirs(proc_dir)
        self.write_file_contents(
            os.path.join(proc_dir, "cpuinfo"), "vendor_id	: AuthenticAMD\n"
        )

    def create_empty_platform_profile(self):
        acpi_dir = os.path.join(self.testbed.get_root_dir(), "sys/firmware/acpi/")
        os.makedirs(acpi_dir)
        self.write_file_contents(os.path.join(acpi_dir, "platform_profile"), "\n")
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile_choices"), "\n"
        )

    def create_platform_profile(self):
        acpi_dir = os.path.join(self.testbed.get_root_dir(), "sys/firmware/acpi/")
        os.makedirs(acpi_dir, exist_ok=True)
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile"), "performance\n"
        )
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile_choices"),
            "low-power balanced performance\n",
        )

    def create_custom_platform_profile(self):
        acpi_dir = os.path.join(self.testbed.get_root_dir(), "sys/firmware/acpi/")
        os.makedirs(acpi_dir, exist_ok=True)
        self.write_file_contents(os.path.join(acpi_dir, "platform_profile"), "custom\n")
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile_choices"),
            "low-power balanced performance custom\n",
        )

    def remove_platform_profile(self):
        acpi_dir = os.path.join(self.testbed.get_root_dir(), "sys/firmware/acpi/")
        os.remove(os.path.join(acpi_dir, "platform_profile_choices"))
        os.remove(os.path.join(acpi_dir, "platform_profile"))
        os.removedirs(acpi_dir)

    def powerprofilesctl_path(self):
        builddir = os.getenv("top_builddir", ".")
        return os.path.join(builddir, "src", "powerprofilesctl")

    def python_coverage_commands(self):
        coverage = os.getenv("PPD_PYTHON_COVERAGE")
        if not coverage:
            return []

        builddir = os.getenv("top_builddir", ".")
        data_file = os.path.join(builddir, "python-coverage", self.id() + ".coverage")
        # We also may need to use "--parallel-mode" if running with
        # meson test --repeat, but this is not a priority for now.
        return [
            coverage,
            "run",
            f"--data-file={data_file}",
            f"--include={builddir}/*",
        ]

    def powerprofilesctl_command(self):
        return self.python_coverage_commands() + [self.powerprofilesctl_path()]

    def assert_eventually(self, condition, message=None, timeout=5000, keep_checking=0):
        """Assert that condition function eventually returns True.

        Timeout is in milliseconds, defaulting to 5000 (5 seconds). message is
        printed on failure.
        """
        if not keep_checking:
            if condition():
                return

        done = False

        def on_timeout_reached():
            nonlocal done
            done = True

        source = GLib.timeout_add(timeout, on_timeout_reached)
        while not done:
            if condition():
                GLib.source_remove(source)
                if keep_checking > 0:
                    self.assert_condition_persists(
                        condition, message, timeout=keep_checking
                    )
                return
            GLib.MainContext.default().iteration(False)

        self.fail(message() if message else f"timed out waiting for {condition}")

    def assert_condition_persists(self, condition, message=None, timeout=1000):
        done = False

        def on_timeout_reached():
            nonlocal done
            done = True

        source = GLib.timeout_add(timeout, on_timeout_reached)
        while not done:
            if not condition():
                GLib.source_remove(source)
                self.fail(
                    message() if message else f"Condition is not persisting {condition}"
                )
            GLib.MainContext.default().iteration(False)

    def assert_file_eventually_contains(
        self, path, contents, timeout=800, keep_checking=0
    ):
        """Asserts that file contents eventually matches expectations"""
        encoded = contents.encode("utf-8")
        return self.assert_eventually(
            lambda: self.read_file_contents(path) == encoded,
            timeout=timeout,
            keep_checking=keep_checking,
            message=lambda: f"file '{path}' does not contain '{contents}', "
            + f"but '{self.read_file_contents(path)}'",
        )

    # pylint: disable=too-many-arguments,unknown-option-value,too-many-positional-arguments
    def assert_sysfs_attr_eventually_is(
        self, device, attribute, contents, timeout=800, keep_checking=0
    ):
        """Asserts that file contents eventually matches expectations"""
        encoded = contents.encode("utf-8")
        return self.assert_eventually(
            lambda: self.read_sysfs_attr(device, attribute) == encoded,
            timeout=timeout,
            keep_checking=keep_checking,
            message=lambda: f"file {device} '{attribute}' does not contain '{contents}', "
            + f"but '{self.read_sysfs_attr(device, attribute)}'",
        )

    def assert_dbus_property_eventually_is(
        self, prop, value, timeout=1200, keep_checking=0
    ):
        """Asserts that a dbus property eventually is what expected"""
        return self.assert_eventually(
            lambda: self.get_dbus_property(prop) == value,
            timeout=timeout,
            keep_checking=keep_checking,
            message=lambda: f"property '{prop}' is not '{value}', but "
            + f"'{self.get_dbus_property(prop)}'",
        )

    def _assert_action_boolean(self, name, value):
        for action in self.get_dbus_property("ActionsInfo"):
            if action["Name"] != name:
                continue
            self.assertEqual(action["Enabled"], value)

    def assert_action_disabled(self, name):
        """Assert that a PPD action is disabled"""
        if self.PP_INTERFACE == "org.freedesktop.UPower.PowerProfiles":
            self._assert_action_boolean(name, False)
        else:
            self.assertNotIn(name, self.get_dbus_property("Actions"))

    def assert_action_enabled(self, name):
        """Assert that a PPD action is enabled"""
        if self.PP_INTERFACE == "org.freedesktop.UPower.PowerProfiles":
            self._assert_action_boolean(name, True)
        else:
            self.assertIn(name, self.get_dbus_property("Actions"))

    #
    # Actual test cases
    #
    def test_dbus_startup_error(self):
        """D-Bus startup error"""

        self.start_daemon()
        daemon_path = [self.daemon_path]
        if os.getenv("PPD_TEST_WRAPPER"):
            daemon_path = os.getenv("PPD_TEST_WRAPPER").split(" ") + daemon_path
        out = subprocess.run(
            daemon_path,
            env={
                "LD_PRELOAD": os.getenv("PPD_LD_PRELOAD")
                + " "
                + os.getenv("LD_PRELOAD")
            },
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        self.assertEqual(
            out.returncode, 1, "power-profile-daemon started but should have failed"
        )
        self.stop_daemon()

    def test_no_performance_driver(self):
        """no performance driver"""

        self.start_daemon()
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")
        self.assertEqual(self.get_dbus_property("PerformanceDegraded"), "")

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 2)
        self.assertEqual(profiles[1]["Driver"], "placeholder")
        self.assertEqual(profiles[1]["PlatformDriver"], "placeholder")
        self.assertEqual(profiles[0]["PlatformDriver"], "placeholder")
        self.assertEqual(profiles[1]["Profile"], "balanced")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        with self.assertRaises(gi.repository.GLib.GError):
            self.set_dbus_property(
                "ActiveProfile", GLib.Variant.new_string("performance")
            )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        with self.assertRaises(gi.repository.GLib.GError):
            cookie = self.call_dbus_method(
                "HoldProfile",
                GLib.Variant("(sss)", ("performance", "testReason", "testApplication")),
            )
            assert cookie

        self.stop_daemon()

    def test_invalid_property(self):
        """Test behavior for requesting an invalid property"""

        self.start_daemon()

        with self.assertRaises(gi.repository.GLib.GError):
            self.get_dbus_property("Foothebar")

    def test_inhibited_property(self):
        """Test that the inhibited property exists"""

        self.create_dytc_device()
        self.create_platform_profile()
        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(self.get_dbus_property("PerformanceInhibited"), "")

    def test_multi_degredation(self):
        """Test handling of degradation from multiple drivers"""
        self.create_dytc_device()
        self.create_platform_profile()

        # Create CPU with preference
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )

        # Create Intel P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "0\n")
        self.write_file_contents(os.path.join(pstate_dir, "turbo_pct"), "1\n")
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_daemon()

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # Degraded CPU
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "1\n")
        self.assert_eventually(
            lambda: self.have_text_in_log("File monitor change happened for ")
        )

        self.assertEqual(
            self.get_dbus_property("PerformanceDegraded"), "high-operating-temperature"
        )

        # Degraded DYTC
        lapmode = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/thinkpad_acpi/dytc_lapmode"
        )
        self.write_file_contents(lapmode, "1\n")
        self.assert_eventually(lambda: self.have_text_in_log("dytc_lapmode is now on"))
        self.assertEqual(
            self.get_dbus_property("PerformanceDegraded"),
            "high-operating-temperature,lap-detected",
        )

    def test_degraded_transition(self):
        """Test that transitions work as expected when degraded"""

        self.create_dytc_device()
        self.create_platform_profile()
        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # Degraded
        lapmode = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/thinkpad_acpi/dytc_lapmode"
        )
        self.write_file_contents(lapmode, "1\n")
        self.assert_eventually(lambda: self.have_text_in_log("dytc_lapmode is now on"))
        self.assertEqual(self.get_dbus_property("PerformanceDegraded"), "lap-detected")
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # Switch to non-performance
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

    def test_intel_pstate(self):
        """Intel P-State driver (no UPower)"""

        # Create 2 CPUs with preferences
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy1/"
        )
        os.makedirs(dir2)
        self.write_file_contents(os.path.join(dir2, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir2, "energy_performance_preference"), "performance\n"
        )

        # Create Intel P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "0\n")
        self.write_file_contents(os.path.join(pstate_dir, "turbo_pct"), "1\n")
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "intel_pstate")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        energy_prefs = os.path.join(dir2, "energy_performance_preference")
        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        self.assert_file_eventually_contains(energy_prefs, "performance")

        # Disable turbo
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "1\n")

        self.assert_eventually(
            lambda: self.have_text_in_log("File monitor change happened for ")
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")
        self.assertEqual(
            self.get_dbus_property("PerformanceDegraded"), "high-operating-temperature"
        )

        self.stop_daemon()

        # Verify that Lenovo DYTC and Intel P-State drivers are loaded
        self.create_platform_profile()
        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "intel_pstate")
        self.assertEqual(profiles[0]["PlatformDriver"], "platform_profile")

    def test_intel_pstate_balance(self):
        """Intel P-State driver (balance)"""

        # Create CPU with preference
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        gov_path = os.path.join(dir1, "scaling_governor")
        self.write_file_contents(gov_path, "performance\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": False},
        )

        self.start_daemon()

        self.assert_file_eventually_contains(gov_path, "powersave")

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "intel_pstate")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        self.assert_file_eventually_contains(
            os.path.join(dir1, "energy_performance_preference"), "balance_performance"
        )

    def test_intel_pstate_reapply_on_resume_from_sleep_disable_logind(self):
        # Create CPU with preference
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        gov_path = os.path.join(dir1, "scaling_governor")
        self.write_file_contents(gov_path, "performance\n")
        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        self.write_file_contents(energy_prefs, "performance\n")
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        _, obj_logind, _ = self.start_dbus_template("logind", {})

        self.start_daemon(["--disable-logind"])
        self.assert_dbus_property_eventually_is(
            "ActiveProfile", "balanced", keep_checking=100
        )

        # Simulate system changing to performance mode just before going to suspend
        self.write_file_contents(energy_prefs, "performance\n")
        self.assert_file_eventually_contains(
            energy_prefs, "performance\n", keep_checking=500
        )

        obj_logind.EmitSignal(
            "org.freedesktop.login1.Manager", "PrepareForSleep", "b", [True]
        )
        self.assert_file_eventually_contains(
            energy_prefs, "performance\n", keep_checking=500
        )

        # Check that on resume the value stays.
        obj_logind.EmitSignal(
            "org.freedesktop.login1.Manager", "PrepareForSleep", "b", [False]
        )

        self.assert_file_eventually_contains(
            energy_prefs, "performance\n", timeout=3000, keep_checking=100
        )

    def test_intel_pstate_reapply_on_resume_from_sleep(self):
        # Create CPU with preference
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        gov_path = os.path.join(dir1, "scaling_governor")
        self.write_file_contents(gov_path, "performance\n")
        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        self.write_file_contents(energy_prefs, "performance\n")
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        _, obj_logind, _ = self.start_dbus_template("logind", {})

        self.start_daemon()
        self.assert_dbus_property_eventually_is(
            "ActiveProfile", "balanced", keep_checking=100
        )

        # Simulate system changing to performance mode just before going to suspend
        self.write_file_contents(energy_prefs, "performance\n")
        self.assert_file_eventually_contains(
            energy_prefs, "performance\n", keep_checking=500
        )

        obj_logind.EmitSignal(
            "org.freedesktop.login1.Manager", "PrepareForSleep", "b", [True]
        )
        self.assert_file_eventually_contains(
            energy_prefs, "performance\n", keep_checking=500
        )

        # Check that on resume the value is reset to the expected one.
        obj_logind.EmitSignal(
            "org.freedesktop.login1.Manager", "PrepareForSleep", "b", [False]
        )

        self.assert_file_eventually_contains(
            energy_prefs, "balance_performance", timeout=3000, keep_checking=100
        )

    def test_intel_pstate_error(self):
        """Intel P-State driver in error state"""

        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        pref_path = os.path.join(dir1, "energy_performance_preference")
        old_umask = os.umask(0o333)
        self.write_file_contents(pref_path, "balance_performance\n")
        os.umask(old_umask)
        # Make file non-writable to root
        self.change_immutable(pref_path, True)

        self.start_daemon()

        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # Error when setting performance mode
        with self.assertRaises(gi.repository.GLib.GError):
            self.set_dbus_property(
                "ActiveProfile", GLib.Variant.new_string("performance")
            )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        self.assert_file_eventually_contains(energy_prefs, "balance_performance\n")

    def test_intel_pstate_passive(self):
        """Intel P-State in passive mode -> placeholder"""

        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )

        # Create Intel P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "0\n")
        self.write_file_contents(os.path.join(pstate_dir, "turbo_pct"), "1\n")
        self.write_file_contents(os.path.join(pstate_dir, "status"), "passive\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 2)
        self.assertEqual(profiles[0]["Driver"], "placeholder")
        self.assertEqual(profiles[0]["PlatformDriver"], "placeholder")
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        self.assert_file_eventually_contains(energy_prefs, "performance\n")

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        self.assert_file_eventually_contains(energy_prefs, "performance\n")

    def test_intel_pstate_passive_with_epb(self):
        """Intel P-State in passive mode (no HWP) with energy_perf_bias"""

        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpu0/power/"
        )
        os.makedirs(dir2)
        self.write_file_contents(os.path.join(dir2, "energy_perf_bias"), "6")

        # Create Intel P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "0\n")
        self.write_file_contents(os.path.join(pstate_dir, "turbo_pct"), "1\n")
        self.write_file_contents(os.path.join(pstate_dir, "status"), "passive\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "intel_pstate")
        self.assertEqual(profiles[0]["PlatformDriver"], "placeholder")
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # Set power-saver mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        energy_perf_bias = os.path.join(dir2, "energy_perf_bias")
        self.assert_file_eventually_contains(energy_perf_bias, "15")

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        self.assert_file_eventually_contains(energy_perf_bias, "0")

    def test_action_blocklist(self):
        """Test action blocklist works"""
        self.start_daemon(["--block-action", "trickle_charge"])
        self.assert_action_disabled("trickle_charge")

    def test_driver_blocklist(self):
        """Test driver blocklist works"""
        # Create 2 CPUs with preferences
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        scaling_governor = os.path.join(dir1, "scaling_governor")
        self.write_file_contents(scaling_governor, "powersave\n")

        prefs1 = os.path.join(dir1, "energy_performance_preference")
        self.write_file_contents(prefs1, "performance\n")

        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy1/"
        )
        os.makedirs(dir2)
        scaling_governor = os.path.join(dir2, "scaling_governor")
        self.write_file_contents(scaling_governor, "powersave\n")
        prefs2 = os.path.join(
            dir2,
            "energy_performance_preference",
        )
        self.write_file_contents(prefs2, "prformance\n")

        # Create AMD P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        # create ACPI platform profile
        self.create_platform_profile()
        profile = os.path.join(
            self.testbed.get_root_dir(), "sys/firmware/acpi/platform_profile"
        )
        self.assertNotEqual(profile, None)

        # block platform profile
        self.start_daemon(["--block-driver", "platform_profile"])
        # Verify that only amd-pstate is loaded
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(profiles[0]["PlatformDriver"], "placeholder")

        self.stop_daemon()

        # block both drivers
        self.start_daemon(
            ["--block-driver", "amd_pstate", "--block-driver", "platform_profile"]
        )
        # Verify that only placeholder is loaded
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 2)
        self.assertEqual(profiles[0]["PlatformDriver"], "placeholder")

    # pylint: disable=too-many-statements
    def test_multi_driver_flows(self):
        """Test corner cases associated with multiple drivers"""

        # Create 2 CPUs with preferences
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        prefs1 = os.path.join(dir1, "energy_performance_preference")
        self.write_file_contents(prefs1, "performance\n")

        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy1/"
        )
        os.makedirs(dir2)
        self.write_file_contents(os.path.join(dir2, "scaling_governor"), "powersave\n")
        prefs2 = os.path.join(dir2, "energy_performance_preference")
        self.write_file_contents(prefs2, "performance\n")

        # Create AMD P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        # create ACPI platform profile
        self.create_platform_profile()
        profile = os.path.join(
            self.testbed.get_root_dir(), "sys/firmware/acpi/platform_profile"
        )

        self.start_daemon()

        # Verify that both drivers are loaded
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(profiles[0]["PlatformDriver"], "platform_profile")

        # test both drivers can switch to power-saver
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        # test both drivers can switch to performance
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # test both drivers can switch to balanced
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # test when CPU driver fails to write
        self.change_immutable(prefs1, True)
        with self.assertRaises(gi.repository.GLib.GError):
            self.set_dbus_property(
                "ActiveProfile", GLib.Variant.new_string("power-saver")
            )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")
        self.assertEqual(
            self.read_sysfs_file("sys/firmware/acpi/platform_profile"), b"balanced"
        )
        self.change_immutable(prefs1, False)

        # test when platform driver fails to write
        self.change_immutable(profile, True)
        with self.assertRaises(gi.repository.GLib.GError):
            self.set_dbus_property(
                "ActiveProfile", GLib.Variant.new_string("power-saver")
            )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # make sure CPU was undone since platform failed
        self.assertEqual(
            self.read_sysfs_file(
                "sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference"
            ),
            b"balance_performance",
        )
        self.assertEqual(
            self.read_sysfs_file(
                "sys/devices/system/cpu/cpufreq/policy1/energy_performance_preference"
            ),
            b"balance_performance",
        )

    # pylint: disable=too-many-statements
    def test_amd_pstate_state_machine(self):
        # Create 2 CPUs with preferences
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy1/"
        )
        os.makedirs(dir2)
        self.write_file_contents(os.path.join(dir2, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir2, "energy_performance_preference"), "performance\n"
        )

        # Create AMD P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)

        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        energy_prefs = os.path.join(dir2, "energy_performance_preference")
        scaling_governor = os.path.join(dir2, "scaling_governor")

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

        self.write_file_contents(os.path.join(pstate_dir, "status"), "passive\n")

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # ensure nothing changed
        self.assert_file_eventually_contains(energy_prefs, "balance_performance")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

    # pylint: disable=too-many-statements
    def test_amd_pstate(self):
        """AMD P-State driver (no UPower)"""

        # Create 2 CPUs with preferences
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy1/"
        )
        os.makedirs(dir2)
        self.write_file_contents(os.path.join(dir2, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir2, "energy_performance_preference"), "performance\n"
        )

        # Create AMD P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)

        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        energy_prefs = os.path.join(dir2, "energy_performance_preference")
        scaling_governor = os.path.join(dir2, "scaling_governor")

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        self.assert_file_eventually_contains(energy_prefs, "performance")
        self.assert_file_eventually_contains(scaling_governor, "performance")

        # Set powersave mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        self.assert_file_eventually_contains(energy_prefs, "power")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

    # pylint: disable=too-many-statements
    def test_amd_pstate_min_freq(self):
        """AMD P-State driver min freq support"""
        # Create 2 CPUs with preferences
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "cpuinfo_min_freq"), "400000\n")
        self.write_file_contents(os.path.join(dir1, "scaling_min_freq"), "400000\n")
        self.write_file_contents(
            os.path.join(dir1, "amd_pstate_lowest_nonlinear_freq"), "1114000\n"
        )
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy1/"
        )
        os.makedirs(dir2)
        self.write_file_contents(os.path.join(dir2, "cpuinfo_min_freq"), "400000\n")
        self.write_file_contents(os.path.join(dir2, "scaling_min_freq"), "400000\n")
        self.write_file_contents(
            os.path.join(dir2, "amd_pstate_lowest_nonlinear_freq"), "1114000\n"
        )
        self.write_file_contents(os.path.join(dir2, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir2, "energy_performance_preference"), "performance\n"
        )

        # Create AMD P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)

        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        energy_prefs = os.path.join(dir2, "energy_performance_preference")
        scaling_governor = os.path.join(dir2, "scaling_governor")
        scaling_min_freq = os.path.join(dir2, "scaling_min_freq")

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")
        self.assert_file_eventually_contains(scaling_governor, "powersave")
        self.assert_file_eventually_contains(scaling_min_freq, "1114000")

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        self.assert_file_eventually_contains(energy_prefs, "performance")
        self.assert_file_eventually_contains(scaling_governor, "performance")
        self.assert_file_eventually_contains(scaling_min_freq, "1114000")

        # Set powersave mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        self.assert_file_eventually_contains(energy_prefs, "power")
        self.assert_file_eventually_contains(scaling_governor, "powersave")
        self.assert_file_eventually_contains(scaling_min_freq, "400000")

    # pylint: disable=too-many-statements
    def test_amd_pstate_boost(self):
        """AMD P-State driver boost support"""

        # Create 2 CPUs with preferences
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "boost"), "1\n")
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy1/"
        )
        os.makedirs(dir2)
        self.write_file_contents(os.path.join(dir2, "boost"), "1\n")
        self.write_file_contents(os.path.join(dir2, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir2, "energy_performance_preference"), "performance\n"
        )

        # Create AMD P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)

        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        energy_prefs = os.path.join(dir2, "energy_performance_preference")
        scaling_governor = os.path.join(dir2, "scaling_governor")
        boost = os.path.join(dir2, "boost")

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        self.assert_file_eventually_contains(energy_prefs, "performance")
        self.assert_file_eventually_contains(scaling_governor, "performance")
        self.assert_file_eventually_contains(boost, "1")

        # Set powersave mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        self.assert_file_eventually_contains(energy_prefs, "power")
        self.assert_file_eventually_contains(scaling_governor, "powersave")
        self.assert_file_eventually_contains(boost, "0")

    def test_amd_pstate_balance(self):
        """AMD P-State driver (balance)"""

        # Create CPU with preference
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        gov_path = os.path.join(dir1, "scaling_governor")
        self.write_file_contents(gov_path, "performance\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": False},
        )

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        # This matches what's written by ppd-driver-amd-pstate.c
        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        scaling_governor = os.path.join(dir1, "scaling_governor")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

    def test_amd_pstate_error(self):
        """AMD P-State driver in error state"""

        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        pref_path = os.path.join(dir1, "energy_performance_preference")
        old_umask = os.umask(0o333)
        self.write_file_contents(pref_path, "balance_performance\n")
        os.umask(old_umask)
        # Make file non-writable to root
        self.change_immutable(pref_path, True)

        self.start_daemon()

        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # Error when setting performance mode
        with self.assertRaises(gi.repository.GLib.GError):
            self.set_dbus_property(
                "ActiveProfile", GLib.Variant.new_string("performance")
            )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        self.assert_file_eventually_contains(energy_prefs, "balance_performance\n")

    def test_amd_pstate_passive(self):
        """AMD P-State in passive mode -> placeholder"""

        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "balance_performance\n"
        )

        # Create AMD P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "passive\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["PlatformDriver"], "placeholder")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # Set performance mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # Shouldn't have updated
        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

    def test_dytc_performance_driver(self):
        """Lenovo DYTC performance driver"""

        self.create_dytc_device()
        self.create_platform_profile()
        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "platform_profile")
        self.assertEqual(profiles[0]["PlatformDriver"], "platform_profile")
        self.assertEqual(profiles[0]["Profile"], "power-saver")
        self.assertEqual(profiles[2]["PlatformDriver"], "platform_profile")
        self.assertEqual(profiles[2]["Profile"], "performance")
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # lapmode detected
        lapmode = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/thinkpad_acpi/dytc_lapmode"
        )
        self.write_file_contents(lapmode, "1\n")
        self.assert_dbus_property_eventually_is("PerformanceDegraded", "lap-detected")
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # Reset lapmode
        self.write_file_contents(lapmode, "0\n")
        self.assert_dbus_property_eventually_is("PerformanceDegraded", "")

        # Performance mode didn't change
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        # Switch to power-saver mode
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_eventually(
            lambda: self.read_sysfs_file("sys/firmware/acpi/platform_profile")
            == b"low-power"
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        # And mimic a user pressing a Fn+H
        platform_profile = os.path.join(
            self.testbed.get_root_dir(), "sys/firmware/acpi/platform_profile"
        )
        self.write_file_contents(platform_profile, "performance\n")
        self.assert_dbus_property_eventually_is("ActiveProfile", "performance")

    def test_fake_driver(self):
        """Test that the fake driver works"""

        os.environ["POWER_PROFILE_DAEMON_FAKE_DRIVER"] = "1"
        self.start_daemon()
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.stop_daemon()

        del os.environ["POWER_PROFILE_DAEMON_FAKE_DRIVER"]
        self.start_daemon()
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 2)

    def test_amd_pstate_upower(self):
        """Switching between balance_power and balance_performance based on battery"""
        # Create 2 CPUs with preferences
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )
        dir2 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy1/"
        )
        os.makedirs(dir2)
        self.write_file_contents(os.path.join(dir2, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir2, "energy_performance_preference"), "performance\n"
        )

        # Create AMD P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/amd_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        _, _, stop_upowerd = self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": True},
        )

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)

        self.assertEqual(profiles[0]["Driver"], "multiple")
        self.assertEqual(profiles[0]["CpuDriver"], "amd_pstate")
        self.assertEqual(profiles[0]["Profile"], "power-saver")

        energy_prefs = os.path.join(dir2, "energy_performance_preference")
        scaling_governor = os.path.join(dir2, "scaling_governor")

        self.assert_file_eventually_contains(energy_prefs, "balance_power")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

        stop_upowerd()

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        _, upowerd_obj, stop_upowerd = self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": False},
        )

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        upowerd_obj.Set("org.freedesktop.UPower", "OnBattery", True)
        self.assert_file_eventually_contains(energy_prefs, "balance_power")

        # Ensure that changing some other property doesn't change the state.
        upowerd_obj.Set("org.freedesktop.UPower", "LidIsClosed", True)
        self.assert_file_eventually_contains(
            energy_prefs, "balance_power", keep_checking=800
        )

        upowerd_obj.Set("org.freedesktop.UPower", "OnBattery", False)
        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        self.stop_daemon()

        # start upower after the daemon
        stop_upowerd()

        self.start_daemon()

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        _, upowerd_obj, _ = self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": False},
        )
        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        upowerd_obj.Set("org.freedesktop.UPower", "OnBattery", True)
        self.assert_file_eventually_contains(energy_prefs, "balance_power")

    def test_amdgpu_dpm_manual(self):
        """Verify AMDGPU dpm power actions avoid manual"""
        amdgpu_dpm = "device/power_dpm_force_performance_level"
        card = self.testbed.add_device(
            "drm",
            "card0",
            None,
            [amdgpu_dpm, "manual\n"],
            ["DEVTYPE", "drm_minor"],
        )
        self.create_amd_apu()

        self.start_daemon()

        self.call_dbus_method(
            "SetActionEnabled", GLib.Variant("(sb)", ("amdgpu_dpm", True))
        )

        self.assert_action_enabled("amdgpu_dpm")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assert_sysfs_attr_eventually_is(card, amdgpu_dpm, "manual")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_sysfs_attr_eventually_is(card, amdgpu_dpm, "manual")

    def test_amdgpu_dpm(self):
        """Verify AMDGPU dpm power actions"""
        amdgpu_dpm = "device/power_dpm_force_performance_level"
        card = self.testbed.add_device(
            "drm",
            "card0",
            None,
            [amdgpu_dpm, "auto\n"],
            ["DEVTYPE", "drm_minor"],
        )
        self.create_amd_apu()

        self.start_daemon()

        self.call_dbus_method(
            "SetActionEnabled", GLib.Variant("(sb)", ("amdgpu_dpm", True))
        )

        self.assert_action_enabled("amdgpu_dpm")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assert_sysfs_attr_eventually_is(card, amdgpu_dpm, "auto")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_sysfs_attr_eventually_is(card, amdgpu_dpm, "low")

    def test_amdgpu_panel_power(self):
        """Verify AMDGPU Panel power actions"""
        amdgpu_panel_power_savings = "amdgpu/panel_power_savings"
        edp = self.testbed.add_device(
            "drm",
            "card1-eDP",
            None,
            ["status", "connected\n", amdgpu_panel_power_savings, "0"],
            ["DEVTYPE", "drm_connector"],
        )

        self.create_amd_apu()

        self.start_daemon()

        # verify it starts off disabled
        self.assert_action_disabled("amdgpu_panel_power")

        # verify it is now enabled
        self.call_dbus_method(
            "SetActionEnabled", GLib.Variant("(sb)", ("amdgpu_panel_power", True))
        )
        self.assert_action_enabled("amdgpu_panel_power")

        # verify it hasn't been updated yet due to missing upower
        self.assert_sysfs_attr_eventually_is(edp, amdgpu_panel_power_savings, "0")

        # start upower and try again
        self.stop_daemon()
        _, obj, _ = self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": True},
        )

        def set_battery_level(percentage):
            obj.SetDeviceProperties(
                "/org/freedesktop/UPower/devices/DisplayDevice",
                {"Percentage": dbus.Double(percentage, variant_level=1)},
            )

        set_battery_level(50)
        self.start_daemon()

        self.call_dbus_method(
            "SetActionEnabled", GLib.Variant("(sb)", ("amdgpu_panel_power", True))
        )

        # verify balanced has it off at half battery
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assert_sysfs_attr_eventually_is(edp, amdgpu_panel_power_savings, "0")

        # verify balanced turned it on when less than third battery
        set_battery_level(29)
        self.assert_sysfs_attr_eventually_is(edp, amdgpu_panel_power_savings, "1")

        # switch to power saver with a large battery, make sure off
        set_battery_level(70)
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_sysfs_attr_eventually_is(edp, amdgpu_panel_power_savings, "0")

        # # set power saver with less than half battery, should turn on
        set_battery_level(49)
        self.assert_sysfs_attr_eventually_is(edp, amdgpu_panel_power_savings, "1")

        # set power saver with very little battery, should turn on at 3
        set_battery_level(15)
        self.assert_sysfs_attr_eventually_is(edp, amdgpu_panel_power_savings, "3")

        # add another device that supports the feature
        edp2 = self.testbed.add_device(
            "drm",
            "card2-eDP",
            None,
            ["status", "connected\n", amdgpu_panel_power_savings, "0"],
            ["DEVTYPE", "drm_connector"],
        )

        # verify power saver got updated for it
        self.assert_sysfs_attr_eventually_is(edp2, amdgpu_panel_power_savings, "3")

        # add another device that supports the feature, but panel is disconnected
        edp3 = self.testbed.add_device(
            "drm",
            "card3-eDP",
            None,
            ["status", "disconnected\n", amdgpu_panel_power_savings, "0"],
            ["DEVTYPE", "drm_connector"],
        )

        # verify power saver didn't get updated for it
        self.assert_sysfs_attr_eventually_is(edp3, amdgpu_panel_power_savings, "0")

    def test_custom_trickle_charge_device(self):
        """Attempt to Trickle power_supply charge type, but already set to Custom"""

        fastcharge = self.testbed.add_device(
            "power_supply",
            "bq24190-charger",
            None,
            ["charge_type", "Custom", "scope", "Device"],
            [],
        )

        self.start_daemon()

        self.assert_action_enabled("trickle_charge")

        # Verify that charge-type stays untouched
        self.assertEqual(self.read_sysfs_attr(fastcharge, "charge_type"), b"Custom")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Custom")
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Custom")

        # verify charge type is touched again
        self.write_sysfs_attr(fastcharge, "charge_type", "Fast")
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Trickle")
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Standard")

        # verify it's not touched again
        self.write_sysfs_attr(fastcharge, "charge_type", "Custom")
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Custom")

        # verify it's not touched again
        self.write_sysfs_attr(fastcharge, "charge_type", "Adaptive")
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Adaptive")

    def test_trickle_charge_system(self):
        """Trickle power_supply charge type"""

        fastcharge = self.testbed.add_device(
            "power_supply",
            "bq24190-charger",
            None,
            ["charge_type", "Trickle", "scope", "System"],
            [],
        )

        self.start_daemon()

        self.assert_action_enabled("trickle_charge")

        # Verify that charge-type stays untouched
        self.assertEqual(self.read_sysfs_attr(fastcharge, "charge_type"), b"Trickle")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.read_sysfs_attr(fastcharge, "charge_type"), b"Trickle")

    def test_trickle_charge_mode_no_change(self):
        """Trickle power_supply charge type"""

        fastcharge = self.testbed.add_device(
            "power_supply",
            "MFi Fastcharge",
            None,
            ["charge_type", "Standard", "scope", "Device"],
            [],
        )

        mtime = self.get_mtime(fastcharge, "charge_type")
        self.start_daemon()

        self.assert_action_enabled("trickle_charge")

        # Verify that charge-type didn't get touched
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Standard")
        self.assertEqual(self.get_mtime(fastcharge, "charge_type"), mtime)

    def test_trickle_charge_mode(self):
        """Trickle power_supply charge type"""

        idevice = self.testbed.add_device(
            "usb",
            "iDevice",
            None,
            [],
            ["ID_MODEL", "iDevice", "DRIVER", "apple-mfi-fastcharge"],
        )
        fastcharge = self.testbed.add_device(
            "power_supply",
            "MFi Fastcharge",
            idevice,
            ["charge_type", "Trickle", "scope", "Device"],
            [],
        )

        self.start_daemon()

        self.assert_action_enabled("trickle_charge")

        # Verify that charge-type got changed to Standard on startup
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Standard")

        # Verify that charge-type got changed to Trickle when power saving
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Trickle")

        # FIXME no performance mode
        # Verify that charge-type got changed to Fast in a non-default, non-power save mode
        # self.set_dbus_property('ActiveProfile', GLib.Variant.new_string('performance'))
        # self.assert_sysfs_attr_eventually_is(fastcharge, "charge_type", "Fast")

    def test_platform_driver_late_load(self):
        """Test that we can handle the platform_profile driver getting loaded late"""
        self.create_empty_platform_profile()
        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 2)

        acpi_dir = os.path.join(self.testbed.get_root_dir(), "sys/firmware/acpi/")
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile_choices"),
            "low-power\nbalanced\nperformance\n",
        )
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile"), "performance\n"
        )

        # Wait for profiles to get reloaded
        self.assert_eventually(lambda: len(self.get_dbus_property("Profiles")) == 3)
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        # Was set in platform_profile before we loaded the drivers
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")
        self.assertEqual(self.get_dbus_property("PerformanceDegraded"), "")

    def test_hp_wmi(self):
        # Uses cool instead of low-power
        acpi_dir = os.path.join(self.testbed.get_root_dir(), "sys/firmware/acpi/")
        os.makedirs(acpi_dir)
        self.write_file_contents(os.path.join(acpi_dir, "platform_profile"), "cool\n")
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile_choices"),
            "cool balanced performance\n",
        )

        self.start_daemon()
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "platform_profile")
        self.assertEqual(profiles[0]["PlatformDriver"], "platform_profile")
        self.assertEqual(profiles[0]["Profile"], "power-saver")
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")
        self.assertEqual(
            self.read_sysfs_file("sys/firmware/acpi/platform_profile"), b"cool"
        )
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        self.assertEqual(
            self.read_sysfs_file("sys/firmware/acpi/platform_profile"), b"cool"
        )

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("performance"))
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assertEqual(
            self.read_sysfs_file("sys/firmware/acpi/platform_profile"), b"balanced"
        )

    def test_quiet(self):
        # Uses quiet instead of low-power
        acpi_dir = os.path.join(self.testbed.get_root_dir(), "sys/firmware/acpi/")
        os.makedirs(acpi_dir)
        self.write_file_contents(os.path.join(acpi_dir, "platform_profile"), "quiet\n")
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile_choices"),
            "quiet balanced balanced-performance performance\n",
        )

        self.start_daemon()
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["Driver"], "platform_profile")
        self.assertEqual(profiles[0]["PlatformDriver"], "platform_profile")
        self.assertEqual(profiles[0]["Profile"], "power-saver")
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")
        self.assertEqual(
            self.read_sysfs_file("sys/firmware/acpi/platform_profile"), b"balanced"
        )
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        self.assertEqual(
            self.read_sysfs_file("sys/firmware/acpi/platform_profile"), b"quiet"
        )

    def test_custom_acpi_platform_profile(self):
        self.create_custom_platform_profile()
        self.start_daemon()
        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(profiles[0]["PlatformDriver"], "platform_profile")

        # make sure PPD overrides it
        self.assertEqual(
            self.read_sysfs_file("sys/firmware/acpi/platform_profile"), b"balanced"
        )

    def test_hold_release_profile(self):
        self.create_platform_profile()
        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)

        cookie = self.call_dbus_method(
            "HoldProfile",
            GLib.Variant("(sss)", ("performance", "testReason", "testApplication")),
        )
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfile") == "performance"
        )
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfileHolds")
            == [
                {
                    "ApplicationId": "testApplication",
                    "Profile": "performance",
                    "Reason": "testReason",
                }
            ]
        )

        released_cookie = None

        def signal_cb(_, sender, signal_name, params):
            nonlocal released_cookie
            if signal_name == "ProfileReleased":
                released_cookie = params

        self.addCleanup(
            self.proxy.disconnect, self.proxy.connect("g-signal", signal_cb)
        )

        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")
        profile_holds = self.get_dbus_property("ActiveProfileHolds")
        self.assertEqual(len(profile_holds), 1)
        self.assertEqual(profile_holds[0]["Profile"], "performance")
        self.assertEqual(profile_holds[0]["Reason"], "testReason")
        self.assertEqual(profile_holds[0]["ApplicationId"], "testApplication")

        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", cookie))
        self.assert_eventually(lambda: released_cookie == cookie)
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfile") == "balanced"
        )
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfileHolds") == []
        )
        profile_holds = self.get_dbus_property("ActiveProfileHolds")
        self.assertEqual(len(profile_holds), 0)
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # When the profile is changed manually, holds should be released a
        self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("performance", "", ""))
        )
        self.assertEqual(len(self.get_dbus_property("ActiveProfileHolds")), 1)
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfile") == "performance"
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")

        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("balanced"))
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfile") == "balanced"
        )
        self.assertEqual(len(self.get_dbus_property("ActiveProfileHolds")), 0)
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # When all holds are released, the last manually selected profile should be activated
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfile") == "power-saver"
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("performance", "", ""))
        )
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfileHolds")
            == [
                {
                    "ApplicationId": "",
                    "Profile": "performance",
                    "Reason": "",
                }
            ]
        )
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfile") == "performance"
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", cookie))
        self.assert_eventually(lambda: released_cookie == cookie)
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfileHolds") == []
        )
        self.assert_eventually(
            lambda: self.changed_properties.get("ActiveProfile") == "power-saver"
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

    def test_launch_arguments_redirection(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        with tempfile.NamedTemporaryFile(mode="+rt") as tmpf:
            subprocess.check_call(
                self.powerprofilesctl_command()
                + [
                    "launch",
                    "sh",
                    "-c",
                    f'echo "$@" > {tmpf.name}',
                    "--",
                    "--foo",
                    "--bar",
                    "-v",
                    "arg",
                ]
            )
            self.assertEqual(tmpf.readlines(), ["--foo --bar -v arg\n"])

    def test_unknown_action(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        with self.assertRaises(subprocess.CalledProcessError):
            tool_cmd = self.powerprofilesctl_command()
            subprocess.check_output(
                tool_cmd + ["hopefully-invalid-action"], stderr=subprocess.PIPE
            )

    def test_unknown_list_argument(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        with self.assertRaises(subprocess.CalledProcessError):
            subprocess.check_output(
                self.powerprofilesctl_command() + ["list", "--invalid-argument"],
                stderr=subprocess.PIPE,
            )

    def test_launch_arguments_invalid(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        with self.assertRaises(subprocess.CalledProcessError):
            tool_cmd = self.powerprofilesctl_command()
            subprocess.check_output(
                tool_cmd + ["--foo-arg", "launch", "true"], stderr=subprocess.PIPE
            )

    def test_launch_with_command_failure(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        tool_cmd = self.powerprofilesctl_command()
        cmd = subprocess.run(tool_cmd + ["launch", "false"], check=False)
        self.assertEqual(cmd.returncode, 1)

        cmd = subprocess.run(tool_cmd + ["launch", "sh", "-c", "exit 55"], check=False)
        self.assertEqual(cmd.returncode, 55)

    def test_launch_with_command_signaled(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        tool_cmd = self.powerprofilesctl_command()
        cmd = subprocess.run(
            tool_cmd + ["launch", "sh", "-c", f"kill -{signal.SIGKILL} $$"], check=False
        )
        self.assertEqual(cmd.returncode, -signal.SIGKILL)

        cmd = subprocess.run(
            tool_cmd + ["launch", "sh", "-c", f"kill -{signal.SIGINT} $$"], check=False
        )
        self.assertEqual(cmd.returncode, -signal.SIGINT)

    def test_vanishing_hold(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        tool_cmd = self.powerprofilesctl_command()
        with subprocess.Popen(
            tool_cmd + ["launch", "-p", "power-saver", "sleep", "3600"],
            stdout=sys.stdout,
            stderr=sys.stderr,
        ) as launch_process:
            self.assertTrue(launch_process)
            time.sleep(1)
            holds = self.get_dbus_property("ActiveProfileHolds")
            self.assertEqual(len(holds), 1)
            hold = holds[0]
            self.assertEqual(hold["Profile"], "power-saver")

            # Make sure to handle vanishing clients
            launch_process.terminate()
            retcode = launch_process.wait()
            self.assertEqual(retcode, -signal.SIGTERM)

        self.assert_eventually(
            lambda: len(self.get_dbus_property("ActiveProfileHolds")) == 0,
            message=lambda: f"Holds are {self.get_dbus_property('ActiveProfileHolds')}",
        )

    def test_launch_sigint_wrapper(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        with subprocess.Popen(
            self.powerprofilesctl_command() + ["launch", "sleep", "3600"],
        ) as launch_process:
            time.sleep(1)
            launch_process.send_signal(signal.SIGINT)
            retcode = launch_process.wait()
            self.assertEqual(retcode, -signal.SIGINT)

    def test_launch_sigabrt_wrapper(self):
        self.create_platform_profile()
        self.start_daemon()
        self.assert_eventually(lambda: self.get_dbus_property("ActiveProfile"))

        with subprocess.Popen(
            self.powerprofilesctl_command() + ["launch", "sleep", "3600"],
        ) as launch_process:
            time.sleep(1)
            launch_process.send_signal(signal.SIGABRT)
            retcode = launch_process.wait()
            self.assertEqual(retcode, -signal.SIGABRT)

    def test_hold_priority(self):
        """power-saver should take priority over performance"""
        self.create_platform_profile()
        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # Test every order of holding and releasing power-saver and performance
        # hold performance and then power-saver, release in the same order
        performance_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("performance", "", ""))
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")
        powersaver_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("power-saver", "", ""))
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", performance_cookie))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", powersaver_cookie))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # hold performance and then power-saver, but release power-saver first
        performance_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("performance", "", ""))
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")
        powersaver_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("power-saver", "", ""))
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", powersaver_cookie))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", performance_cookie))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # hold power-saver and then performance, release in the same order
        powersaver_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("power-saver", "", ""))
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        performance_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("performance", "", ""))
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", powersaver_cookie))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", performance_cookie))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        # hold power-saver and then performance, but release performance first
        powersaver_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("power-saver", "", ""))
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        performance_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("performance", "", ""))
        )
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", performance_cookie))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        self.call_dbus_method("ReleaseProfile", GLib.Variant("(u)", powersaver_cookie))
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

    def test_save_profile(self):
        """save profile across runs"""

        self.create_platform_profile()

        self.start_daemon()
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.stop_daemon()

        # sys.stderr.write('\n-------------- config file: ----------------\n')
        # with open(self.testbed.get_root_dir() + '/' + 'ppd_test_conf.ini') as tmpf:
        #   sys.stderr.write(tmpf.read())
        # sys.stderr.write('------------------------------\n')

        self.start_daemon()
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")
        # Programmatically set profile aren't saved
        performance_cookie = self.call_dbus_method(
            "HoldProfile", GLib.Variant("(sss)", ("performance", "", ""))
        )
        self.assertTrue(performance_cookie)
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "performance")
        self.stop_daemon()

        self.start_daemon()
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

    def test_save_deferred_load(self):
        """save profile across runs, but kernel driver loaded after start"""

        self.create_platform_profile()
        self.start_daemon()
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")
        self.set_dbus_property("ActiveProfile", GLib.Variant.new_string("power-saver"))
        self.stop_daemon()
        self.remove_platform_profile()

        # We could verify the contents of the configuration file here

        self.create_empty_platform_profile()
        self.start_daemon()
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        acpi_dir = os.path.join(self.testbed.get_root_dir(), "sys/firmware/acpi/")
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile_choices"),
            "low-power\nbalanced\nperformance\n",
        )
        self.write_file_contents(
            os.path.join(acpi_dir, "platform_profile"), "performance\n"
        )

        self.assert_dbus_property_eventually_is("ActiveProfile", "power-saver")

    def test_not_allowed_profile(self):
        """Check that we get errors when trying to change a profile and not allowed"""

        self.obj_polkit.SetAllowed(dbus.Array([], signature="s"))
        self.start_daemon()
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        proxy = Gio.DBusProxy.new_sync(
            self.dbus,
            Gio.DBusProxyFlags.DO_NOT_AUTO_START,
            None,
            self.PP,
            self.PP_PATH,
            "org.freedesktop.DBus.Properties",
            None,
        )
        with self.assertRaises(gi.repository.GLib.GError) as error:
            proxy.Set(
                "(ssv)",
                self.PP,
                "ActiveProfile",
                GLib.Variant.new_string("power-saver"),
            )
        self.assertIn("AccessDenied", str(error.exception))

    def test_not_allowed_hold(self):
        """Check that we get an error when trying to hold a profile and not allowed"""

        self.obj_polkit.SetAllowed(dbus.Array([], signature="s"))
        self.start_daemon()
        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        with self.assertRaises(gi.repository.GLib.GError) as error:
            self.call_dbus_method(
                "HoldProfile", GLib.Variant("(sss)", ("performance", "", ""))
            )
        self.assertIn("AccessDenied", str(error.exception))

        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")
        self.assertEqual(len(self.get_dbus_property("ActiveProfileHolds")), 0)

    def test_get_version_prop(self):
        """Checks that the version property is advertised"""
        self.start_daemon()
        self.assertTrue(self.get_dbus_property("Version"))

    def test_intel_pstate_disabled_upower(self):
        # Create CPU with preference
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )

        # Create Intel P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "1\n")
        self.write_file_contents(os.path.join(pstate_dir, "turbo_pct"), "0\n")
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        _, _, stop_upowerd = self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": False},
        )

        self.start_daemon(["--disable-upower"])

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(self.get_dbus_property("PerformanceDegraded"), "")

        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        scaling_governor = os.path.join(dir1, "scaling_governor")

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

        stop_upowerd()

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

    def test_intel_pstate_upower(self):
        # Create CPU with preference
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )

        # Create Intel P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "1\n")
        self.write_file_contents(os.path.join(pstate_dir, "turbo_pct"), "0\n")
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        _, _, stop_upowerd = self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": True},
        )

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(self.get_dbus_property("PerformanceDegraded"), "")

        energy_prefs = os.path.join(dir1, "energy_performance_preference")
        scaling_governor = os.path.join(dir1, "scaling_governor")

        self.assert_file_eventually_contains(energy_prefs, "balance_power")
        self.assert_file_eventually_contains(scaling_governor, "powersave")

        stop_upowerd()

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        _, upowerd_obj, stop_upowerd = self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": False},
        )

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        upowerd_obj.Set("org.freedesktop.UPower", "OnBattery", True)
        self.assert_file_eventually_contains(energy_prefs, "balance_power")

        upowerd_obj.Set("org.freedesktop.UPower", "OnBattery", False)
        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        self.stop_daemon()

        # start upower after the daemon
        stop_upowerd()

        self.start_daemon()

        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        _, upowerd_obj, _ = self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": False},
        )
        self.assert_file_eventually_contains(energy_prefs, "balance_performance")

        upowerd_obj.Set("org.freedesktop.UPower", "OnBattery", True)
        self.assert_file_eventually_contains(energy_prefs, "balance_power")

    def test_intel_pstate_noturbo(self):
        """Intel P-State driver (balance)"""

        # Create CPU with preference
        dir1 = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/cpufreq/policy0/"
        )
        os.makedirs(dir1)
        self.write_file_contents(os.path.join(dir1, "scaling_governor"), "powersave\n")
        self.write_file_contents(
            os.path.join(dir1, "energy_performance_preference"), "performance\n"
        )

        # Create Intel P-State configuration
        pstate_dir = os.path.join(
            self.testbed.get_root_dir(), "sys/devices/system/cpu/intel_pstate"
        )
        os.makedirs(pstate_dir)
        self.write_file_contents(os.path.join(pstate_dir, "no_turbo"), "1\n")
        self.write_file_contents(os.path.join(pstate_dir, "turbo_pct"), "0\n")
        self.write_file_contents(os.path.join(pstate_dir, "status"), "active\n")

        self.start_daemon()

        profiles = self.get_dbus_property("Profiles")
        self.assertEqual(len(profiles), 3)
        self.assertEqual(self.get_dbus_property("PerformanceDegraded"), "")

    def test_powerprofilesctl_configure_battery_aware_command(self):
        """Check powerprofilesctl configure-battery-aware command works"""

        self.start_dbus_template(
            "upower",
            {"DaemonVersion": "0.99", "OnBattery": False},
        )

        self.start_daemon()

        # verify argument is required
        cmd = subprocess.run(
            self.powerprofilesctl_command() + ["configure-battery-aware"],
            capture_output=True,
            check=False,
        )
        self.assertEqual(cmd.returncode, 1)

        # check possible arguments
        for key, value in {"--disable": "False", "--enable": "True"}.items():
            cmd = subprocess.run(
                self.powerprofilesctl_command() + ["configure-battery-aware", key],
                capture_output=True,
                check=True,
            )
            self.assertEqual(cmd.returncode, 0)

            cmd = subprocess.run(
                self.powerprofilesctl_command() + ["query-battery-aware"],
                capture_output=True,
                check=True,
            )
            self.assertEqual(cmd.returncode, 0)
            self.assertIn(
                f"Dynamic changes from charger and battery events: {value}",
                cmd.stdout.decode("utf-8"),
            )

        # make sure can't be enabled twice
        cmd = subprocess.run(
            self.powerprofilesctl_command() + ["configure-battery-aware", "--enable"],
            capture_output=True,
            check=False,
        )
        self.assertEqual(cmd.returncode, 1)

    def test_powerprofilesctl_configure_action_command(self):
        """Check powerprofilesctl configure-action command works"""

        self.start_daemon()

        for key, value in {"--disable": "False", "--enable": "True"}.items():
            with self.subTest(flag=key):
                cmd = subprocess.run(
                    self.powerprofilesctl_command()
                    + ["configure-action", "trickle_charge", key],
                    capture_output=True,
                    check=True,
                )
            self.assertEqual(cmd.returncode, 0)
            self.assertIn(
                f"action: trickle_charge, enable: {value}", cmd.stdout.decode("utf-8")
            )

    def test_powerprofilesctl_list_actions_command(self):
        """Check powerprofilesctl list-actions command works"""

        self.start_daemon()

        cmd = subprocess.run(
            self.powerprofilesctl_command() + ["list-actions"],
            capture_output=True,
            check=True,
        )
        self.assertEqual(cmd.returncode, 0)
        self.assertIn("trickle_charge", cmd.stdout.decode("utf-8"))

    def test_powerprofilesctl_version_command(self):
        """Check powerprofilesctl version command works"""

        self.start_daemon()

        cmd = subprocess.run(self.powerprofilesctl_command() + ["version"], check=True)
        self.assertEqual(cmd.returncode, 0)

    def test_powerprofilesctl_list_command(self):
        """Check powerprofilesctl list command works"""

        self.start_daemon()

        tool_cmd = self.powerprofilesctl_command()
        cmd = subprocess.run(tool_cmd + ["list"], capture_output=True, check=True)
        self.assertEqual(cmd.returncode, 0)
        self.assertIn("* balanced", cmd.stdout.decode("utf-8"))

    def test_powerprofilesctl_set_get_commands(self):
        """Check powerprofilesctl set/get command works"""

        self.start_daemon()

        self.assertEqual(self.get_dbus_property("ActiveProfile"), "balanced")

        tool_cmd = self.powerprofilesctl_command()
        cmd = subprocess.run(tool_cmd + ["get"], capture_output=True, check=True)
        self.assertEqual(cmd.returncode, 0)
        self.assertEqual(cmd.stdout, b"balanced\n")

        cmd = subprocess.run(
            tool_cmd + ["set", "power-saver"], capture_output=True, check=True
        )
        self.assertEqual(cmd.returncode, 0)

        self.assertEqual(self.get_dbus_property("ActiveProfile"), "power-saver")

        cmd = subprocess.run(tool_cmd + ["get"], capture_output=True, check=True)
        self.assertEqual(cmd.returncode, 0)
        self.assertEqual(cmd.stdout, b"power-saver\n")

    def test_powerprofilesctl_error(self):
        """Check that powerprofilesctl returns 1 rather than an exception on error"""

        tool_cmd = self.powerprofilesctl_command()
        with self.assertRaises(subprocess.CalledProcessError) as error:
            subprocess.check_output(
                tool_cmd + ["list"], stderr=subprocess.PIPE, universal_newlines=True
            )
        self.assertNotIn("Traceback", error.exception.stderr)

        with self.assertRaises(subprocess.CalledProcessError) as error:
            subprocess.check_output(
                tool_cmd + ["get"], stderr=subprocess.PIPE, universal_newlines=True
            )
        self.assertNotIn("Traceback", error.exception.stderr)

        with self.assertRaises(subprocess.CalledProcessError) as error:
            subprocess.check_output(
                tool_cmd + ["set", "not-a-profile"],
                stderr=subprocess.PIPE,
                universal_newlines=True,
            )
        self.assertNotIn("Traceback", error.exception.stderr)

        with self.assertRaises(subprocess.CalledProcessError) as error:
            subprocess.check_output(
                tool_cmd + ["list-holds"],
                stderr=subprocess.PIPE,
                universal_newlines=True,
            )
        self.assertNotIn("Traceback", error.exception.stderr)

        with self.assertRaises(subprocess.CalledProcessError) as error:
            subprocess.check_output(
                tool_cmd + ["launch", "-p", "power-saver", "sleep", "1"],
                stderr=subprocess.PIPE,
                universal_newlines=True,
            )
        self.assertNotIn("Traceback", error.exception.stderr)

        self.start_daemon()
        with self.assertRaises(subprocess.CalledProcessError) as error:
            subprocess.check_output(
                tool_cmd + ["set", "not-a-profile"],
                stderr=subprocess.PIPE,
                universal_newlines=True,
            )
        self.assertNotIn("Traceback", error.exception.stderr)

    #
    # Helper methods
    #

    @classmethod
    def _props_to_str(cls, properties):
        """Convert a properties dictionary to uevent text representation."""

        prop_str = ""
        if properties:
            for key, val in properties.items():
                prop_str += f"{key}={val}\n"
        return prop_str


class LegacyDBusNameTests(Tests):
    """This will repeats all the tests in the Tests class using the legacy dbus name"""

    PP = "net.hadess.PowerProfiles"
    PP_PATH = "/net/hadess/PowerProfiles"
    PP_INTERFACE = "net.hadess.PowerProfiles"

    def test_vanishing_hold(self):
        # Let's not block because of this CI failure, the test isn't relying on
        # the old name anyways.
        pass

    def test_amdgpu_dpm(self):
        amdgpu_dpm = "device/power_dpm_force_performance_level"
        self.testbed.add_device(
            "drm",
            "card0",
            None,
            [amdgpu_dpm, "auto\n"],
            ["DEVTYPE", "drm_minor"],
        )
        self.create_amd_apu()

        self.start_daemon()

        # verify can't enable it on legacy interface
        with self.assertRaises(gi.repository.GLib.GError) as error:
            self.call_dbus_method(
                "SetActionEnabled", GLib.Variant("(sb)", ("amdgpu_dpm", True))
            )
        self.assertIn("UnknownMethod", str(error.exception))

    def test_amdgpu_dpm_manual(self):
        # should fail same way as test_amdgpu_dpm
        self.test_amdgpu_dpm()

    def test_amdgpu_panel_power(self):
        amdgpu_panel_power_savings = "amdgpu/panel_power_savings"
        self.testbed.add_device(
            "drm",
            "card1-eDP",
            None,
            ["status", "connected\n", amdgpu_panel_power_savings, "0"],
            ["DEVTYPE", "drm_connector"],
        )

        self.create_amd_apu()

        self.start_daemon()

        # verify it starts off disabled
        self.assert_action_disabled("amdgpu_panel_power")

        # verify can't enable it on legacy interface
        with self.assertRaises(gi.repository.GLib.GError) as error:
            self.call_dbus_method(
                "SetActionEnabled", GLib.Variant("(sb)", ("amdgpu_panel_power", True))
            )
        self.assertIn("UnknownMethod", str(error.exception))


if __name__ == "__main__":
    # run ourselves under umockdev
    if "umockdev" not in os.environ.get("LD_PRELOAD", ""):
        os.execvp("umockdev-wrapper", ["umockdev-wrapper", sys.executable] + sys.argv)

    prog = unittest.main(exit=False)
    if prog.result.errors or prog.result.failures:
        sys.exit(1)

    # Translate to skip error
    if prog.result.testsRun == len(prog.result.skipped):
        sys.exit(77)
