Note: latest version of this file is at:
https://gitlab.freedesktop.org/xdg/shared-mime-info/-/blob/master/CONTRIBUTING.md

A few ground rules for people interested in adding new mime-types.

* Mime-types/file formats internal to one application and never seen by end
  users don't generally need a mime-type definition at all. If one is needed
  by the application for internal purposes, they should only be added to a
  private .xml file and be bundled with the application itself
* A file type which is visible to end users but only used by one
  application is still useful to have in a shared database. Defining
  a type here allows the user to identify downloads and attachments
  without needing the application installed.

* Mime-types used should be IANA registered mime-types when possible
* When old mime-types become registered, the new definition should
  include an alias for the old mime-type

* Magic offset must be as small as possible. For example, the worst case
  scenario for ISO images is 32k inside the file. This is too big for a sniff
  buffer, especially on remote locations. Avoid those.

* New entries or modifications should include a test case (see below)
* No commits should be done that break the test suite, or the test suite
  test in question should be amended, and reason for the changes clearly
  documented in the commit message

Translations
------------

Translations should go through Transifex.com, and the freedesktop.org team:
https://explore.transifex.com/freedesktop/shared-mime-info/

Filing bugs
-----------

Bugs can be filed at:
https://gitlab.freedesktop.org/xdg/shared-mime-info/issues

Merge requests are required for new mime-types and should include one or
more test files to be added to the test suite.

Test suite
----------

The shared-mime-info tests depend on xdgmime [1]. This is taken care of as a
git submodule and as a meson subproject. Run `meson subprojects update` or
`git submodule update --init` after cloning the shared-mime-info repo. If you
wish to keep the submodule automatically up to date on pull,
`git config submodule.recurse true` in the shared-mime-info dir helps with that.

xdgmime will be built automatically by the meson build system for
shared-mime-info.

Meson’s subprojects system[2] requires the xdgmime directory to be in
`subprojects/xdgmime`. An alternate path to xdgmime can be configured by
installing a pkg-config file for xdgmime which points to the desired location.

As part of the tests, `./xdgmime/src/test-mime-data` is run against
`tests/mime-detection/list`. The format of the file is:
`<testcase filename> <expected mime-type> <expected failures>`

The expected failures is whether matching the file with the mime-type would
fail when matched by file, data or name. "x" indicates expected failure, "o"
indicates expected success. Trailing "o"s can be omitted.

See the top of the `tests/mime-detection/list` file for syntax details.

You can also temporarily print the results of `test-mime-data` by putting your
test files in the `staging-tests/` sub-directory.

[1]: Repository details at:
https://gitlab.freedesktop.org/xdg/xdgmime
[2]: https://mesonbuild.com/Subprojects.html

Releasing
---------

- Run `ninja shared-mime-info-pot`, `./po/check_translations.sh` and
  `./po/update_translations.sh` before release
- Commit and push to shared-mime-info gitlab
- Update version number in `meson.build`
- Add changelog to `NEWS`
     git log --pretty=format:'* %s' <prevtag>..
- Commit, make a merge request, wait for CI to ensure `meson dist` works, then merge it.
- Tag and push the tag
- Go to https://gitlab.freedesktop.org/xdg/shared-mime-info/-/releases and click "New release"
  - Fill in version number (3 times), paste NEWS, confirm

Updating the spec on the website
--------------------------------

- update https://gitlab.freedesktop.org/xdg/xdg-specs/blob/master/web-export/specs.idx
