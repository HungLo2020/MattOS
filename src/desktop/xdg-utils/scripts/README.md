# xdg-utils - Scripts and Documentation

The actual scripts are generated from `xdg-*.in`,
[xdg-utils-common.in](xdg-utils-common.in) and
[desc/xdg-*.xml](desc) which contains the
command line descriptions.

These are the files that you want to edit.
To add a new script, you must also provide 
a `desc/xdg-*.xml` file.

**Do not make changes to the generated scripts themselves!**

---

Use `make scripts-clean` to delete all generated files and use
`make scripts` to re-generate them.

The manual files in [man/](man) and [html/](html)
are also generated from the [desc/xdg-*.xml](desc) files.

Use `make scripts html man` to update all generated files

Use `make release` to remove everything but the generated files.

## Writing Documentation

The files in [desc/](desc) make use of the [DocBook format](https://tdg.docbook.org/).

If you want to use more recent tags please update to the minimum version of DocBook that is required by changing the `DOCTYPE`.

Exmaple:

You want to use the `<code>` tag which requires DocBook 4.3.

Replace the lines …

```xml
<!DOCTYPE refentry PUBLIC "-//OASIS//DTD DocBook XML V4.1.2//EN"
    "http://www.oasis-open.org/docbook/xml/4.1.2/docbookx.dtd" [
]>
```

… with:

```xml
<!DOCTYPE refentry PUBLIC "-//OASIS//DTD DocBook XML V4.3//EN"
    "http://www.oasis-open.org/docbook/xml/4.3/docbookx.dtd" [
]>
```

