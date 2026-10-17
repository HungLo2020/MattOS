# xdg-utils

The xdg-utils package is a set of scripts that provide
basic desktop integration functions for any Free Desktop
on Linux, the BSDs and even partially on MacOS and WSL.

They are intended to provide a set of defacto standards.  This
means that:

* Third party software developers can rely on these xdg-utils
  for all of their simple integration needs.

* Developers of desktop environments can make sure that their
  environments are well supported

  If a desktop developer wants to be certain that their environment
  functions with all third party software, then can simply
  make sure that these utilities work properly in their environment.

  This will hopefully mean that 'third tier' window managers
  such as XFCE and Blackbox can reach full parity with Gnome and KDE
  in terms of third party ISV support.

* Distribution vendors can provide custom versions of these utilities

  If a distribution vendor wishes to have unusual systems,
  they can provide custom scripts, and the third party software
  should still continue to work.

Documentation is mostly in the maual pages and on the [freedesktop.org wiki](https://www.freedesktop.org/wiki/Software/xdg-utils/).

## Overview


The following tools are included in xdg-utils 1.2:

* `xdg-desktop-menu` - Install desktop menu items
* `xdg-desktop-icon` - Install icons to the desktop
* `xdg-icon-resource` - Install icon resources
* `xdg-mime` - Query information about file type handling and install descriptions for new file types
* `xdg-open` - Open a file or URL in the user's preferred application
* `xdg-email` - Send mail using the user's preferred e-mail composer
* `xdg-realpath` - Canonicalize filenames (new in 1.2)
* `xdg-screensaver` - Control the screensaver


## Building

While the xdg-utils are shellscripts they are not ready to be used as is.

Run `make` in the root directory of this repository to build [scripts and documentation](scripts).

### Installing

<b>Note:</b> If you just want to use the xdg-utils please use the version packaged in your distribution unless you have a reason not to do so.

You can optionally choose to install the scripts
to a target directory.  To do this, you could issue
the following commands:

```sh
./configure [--prefix=<your-place-here>]
make install
```

That would cause the scripts to be installed to `<your-place-here>/bin`


## Use in (Custom) Install Scripts

Please consider making yourself familiar with how to package for a given distribution
and the recommend tooling before writing a custom installer.
Even a beinner level package is in most cases better than a "run me as root" install script.
(especially if you support only one or a handful of distributions anyway)

That said …

Although we expect that these scripts will generally come as part
of the operating system, we recommend that you package the scripts
that your application needs along with your product as a fallback.

For this purpose please obtain the original version of the xdg-utils from
https://www.freedesktop.org/wiki/Software/xdg-utils/.
The xdg-utils scripts that are distributed by operating systems vendors
may have been tuned for their particular operating system
and may not work on the same broad variety
of operating systems as the original version.

We recommend that you place these scripts in a directory, and
then add that directory to the end of the `PATH`.

---

So, let's say that you're writing your post installation script,
and you want to create a menu on any xdg-util compliant environment.

Let's further assume that you've just installed to `$INSTALL_DIR`,
and that your menu desktop file is in `$INSTALL_DIR/desktop/icon.desktop`.

Finally, let's say that you've included the xdg-utils package in your installation
in `$INSTALL_DIR/xdg-utils`.


Then a simple post install script could look like this:

```sh
export PATH="$PATH:$INSTALL_DIR/xdg-utils"
xdg-desktop-menu install "$INSTALL_DIR/mycompany-myapp.desktop"
```

And now your product has a menu on any XDG compliant desktop!

---

Note that we strongly recommend using this method - that is,
putting your copy of the xdg-utils at the end of the `PATH`,
and then invoking them without a specific path name.

That will allow your users and their system providers to
use custom versions of the xdg-utils to adjust for system specific
differences.

If you wish to absolutely force the issue and only use the versions
you shipped, you could instead hard code the path to the version
you bundle with your application.  We strongly recommend against
this, as it will make your product obsolete more quickly than is
necessary.
