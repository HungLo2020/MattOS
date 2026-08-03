/*
  File: setfattr.c
  (Linux Extended Attributes)

  Copyright (C) 2001-2002 Andreas Gruenbacher <andreas.gruenbacher@gmail.com>
  Copyright (C) 2001-2002 Silicon Graphics, Inc.  All Rights Reserved.

  This program is free software: you can redistribute it and/or modify it
  under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 2 of the License, or
  (at your option) any later version.

  This program is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  GNU General Public License for more details.

  You should have received a copy of the GNU General Public License
  along with this program.  If not, see <http://www.gnu.org/licenses/>.
*/

#include "config.h"

#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <getopt.h>
#include <locale.h>
#include <ctype.h>
#include <libgen.h>
#include <sys/xattr.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include "openat2.h"

#include "xattrat_compat.h"
#include "misc.h"

#define CMD_LINE_OPTIONS "n:x:v:hP"
#define CMD_LINE_SPEC1 "{-n name} [-v value] [-h] file..."
#define CMD_LINE_SPEC2 "{-x name} [-h] file..."
#define CMD_LINE_SPEC3 "[-hP] --restore=file"

static const struct option long_options[] = {
	{ "name",		1, 0, 'n' }, 
	{ "remove",		1, 0, 'x' },
	{ "value",		1, 0, 'v' },
	{ "no-dereference",	0, 0, 'h' },
	{ "physical",		0, 0, 'P' },
	{ "restore",		1, 0, 'B' },
	{ "raw",		0, 0, CHAR_MAX + 1 },
	{ "version",		0, 0, 'V' },
	{ "help",		0, 0, 'H' },
	{ NULL,			0, 0, 0 }
};

static char *opt_name;  /* attribute name to set */
static char *opt_value;  /* attribute value */
static int opt_set;  /* set an attribute */
static int opt_remove;  /* remove an attribute */
static int opt_restore;  /* restore has been run */
static int opt_deref = 1;  /* dereference symbolic links */
static int opt_raw;  /* attribute value is not encoded */
static int opt_physical;

static int had_errors;
static const char *progname;

static int do_set(int dirfd, const char *pathname, const char *fullpath,
		  const char *name, const char *value);
static const char *decode(const char *value, size_t *size);
static int hex_digit(char c);
static int base64_digit(char c);

static const char *strerror_ea(int err)
{
#ifdef __linux__
	/* The Linux kernel does not define ENOATTR, but maps it to ENODATA. */
	if (err == ENODATA)
		return _("No such attribute");
#endif
	return strerror(err);
}

static const char *xquote(const char *str, const char *quote_chars)
{
	const char *q = quote(str, quote_chars);
	if (q == NULL) {
		fprintf(stderr, "%s: %s\n", progname, strerror(errno));
		exit(1);
	}
	return q;
}

static int restore(const char *filename)
{
	char *fullpath = NULL;
	size_t fullpath_size = 0;
	FILE *file;
	char *l;
	int line = 0, backup_line, status = 0;
	int dirfd = -1;
	char *pathname, *dirname = NULL;
#ifdef UNSAFE_RESTORE_WARNINGS
	static int non_physical_restore_warning;
	static int deref_restore_warning;
#endif

	if (strcmp(filename, "-") == 0)
		file = stdin;
	else {
		file = fopen(filename, "r");
		if (file == NULL) {
				fprintf(stderr, "%s: %s: %s\n",
					progname, filename, strerror_ea(errno));
				return 1;
		}
	}

	for(;;) {
		backup_line = line;
		while ((l = next_line(file)) != NULL && *l == '\0')
			line++;
		if (l == NULL)
			break;
		line++;
		if (strncmp(l, "# file: ", 8) != 0) {
			if (file != stdin) {
				fprintf(stderr, _("%s: %s: No filename found "
				                  "in line %d, aborting\n"),
					progname, filename, backup_line);
			} else {
				fprintf(stderr, _("%s: No filename found in "
			                          "line %d of standard input, "
						  "aborting\n"),
					  progname, backup_line);
			}
			status = 1;
			goto cleanup;
		} else
			l += 8;
		l = unquote(l);
		if (high_water_alloc((void **)&fullpath, &fullpath_size, strlen(l)+1)) {
			perror(progname);
			status = 1;
			goto cleanup;
		}
		strcpy(fullpath, l);

#ifdef UNSAFE_RESTORE_WARNINGS
		if (!opt_physical && !non_physical_restore_warning) {
			fprintf(stderr,
				_("Warning: option --restore=file is unsafe "
				  "without option -P (--physical) as it "
				  "traverses symbolic links in pathnames\n"));
			non_physical_restore_warning = 1;
		}
		if (opt_deref && !deref_restore_warning) {
			fprintf(stderr,
				_("Warning: option --restore=file is unsafe "
				  "without option -h (--no-dereference) as it "
				  "dereferences symbolic link pathnames\n"));
			deref_restore_warning = 1;
		}
#endif

		/* find the last pathname component */
		pathname = fullpath + strlen(fullpath);
		while (pathname > fullpath && pathname[-1] == '/')
			pathname--;
		while (pathname > fullpath && pathname[-1] != '/')
			pathname--;

		if (opt_physical && pathname != fullpath) {
			dirname = malloc(pathname - fullpath + 1);
			if (dirname == NULL) {
				fprintf(stderr, "%s: %s\n", progname,
					strerror(errno));
				status = 1;
				goto cleanup;
			}
			memcpy(dirname, fullpath, pathname - fullpath);
			dirname[pathname - fullpath] = '\0';
#ifdef USE_OPENAT2
			struct open_how how = {
				.flags = O_PATH | O_DIRECTORY,
				.resolve = RESOLVE_NO_SYMLINKS,
			};

			dirfd = openat2(AT_FDCWD, dirname, &how, sizeof(how));
#else
			errno = ENOSYS;
			dirfd = -1;
#endif
			if (dirfd == -1) {
				fprintf(stderr,
					_("%s: lookup of directory %s without "
					  "following symlinks: %s\n"),
					progname, dirname, strerror(errno));
				status = 1;
				goto cleanup;
			}
		} else {
			pathname = fullpath;
			dirfd = AT_FDCWD;
		}

		while ((l = next_line(file)) != NULL && *l != '\0') {
			char *name = l, *value = strchr(l, '=');
			line++;
			if (value)
				*value++ = '\0';
			status = do_set(dirfd, pathname, fullpath,
					unquote(name), value);
		}
		if (dirfd != -1 && dirfd != AT_FDCWD) {
			close(dirfd);
			dirfd = -1;
		}
		free(dirname);
		dirname = NULL;
		if (l == NULL)
			break;
		line++;
	}
	if (!feof(file)) {
		fprintf(stderr, "%s: %s: %s\n", progname, filename,
			strerror(errno));
		if (!status)
			status = 1;
	}

cleanup:
	if (dirfd != -1 && dirfd != AT_FDCWD) {
		close(dirfd);
		dirfd = -1;
	}
	free(dirname);
	free(fullpath);
	if (file != stdin)
		fclose(file);
	if (status)
		had_errors++;
	return status;
}

static void help(void)
{
	printf(_("%s %s -- set extended attributes\n"), progname, VERSION);
	printf(_("Usage: %s %s\n"), progname, CMD_LINE_SPEC1);
	printf(_("       %s %s\n"), progname, CMD_LINE_SPEC2);
	printf(_("       %s %s\n"), progname, CMD_LINE_SPEC3);
	printf(_(
"  -n, --name=name         set the value of the named extended attribute\n"
"  -x, --remove=name       remove the named extended attribute\n"
"  -v, --value=value       use value as the attribute value\n"
"  -h, --no-dereference    do not dereference symbolic links\n"
"  -P, --physical          do not traverse symbolic links during a restore\n"
"      --restore=file      restore extended attributes\n"
"      --raw               attribute value is not encoded\n"
"      --version           print version and exit\n"
"      --help              this help text\n"));
}

int main(int argc, char *argv[])
{
	enum { UNDEFINED_MODE, SET_MODE, RESTORE_MODE } mode = UNDEFINED_MODE;
	char **restore_args = NULL;
	int opt;

	progname = basename(argv[0]);

	setlocale(LC_CTYPE, "");
	setlocale(LC_MESSAGES, "");
	bindtextdomain(PACKAGE, LOCALEDIR);
	textdomain(PACKAGE);

	while ((opt = getopt_long(argc, argv, CMD_LINE_OPTIONS,
		                  long_options, NULL)) != -1) {
		switch(opt) {
			case 'n':  /* attribute name */
				if (mode == RESTORE_MODE)
					goto synopsis;
				mode = SET_MODE;
				if (opt_name || opt_remove)
					goto synopsis;
				opt_name = optarg;
				opt_set = 1;
				break;

			case 'h':  /* set attribute on symlink itself */
				opt_deref = 0;
				break;

			case 'v':  /* attribute value */
				if (mode == RESTORE_MODE)
					goto synopsis;
				mode = SET_MODE;
				if (opt_value || opt_remove)
					goto synopsis;
				opt_value = optarg;
				break;

			case CHAR_MAX + 1:
				if (mode == RESTORE_MODE)
					goto synopsis;
				mode = SET_MODE;
				opt_raw = 1;
				break;

			case 'x':  /* remove attribute */
				if (mode == RESTORE_MODE)
					goto synopsis;
				mode = SET_MODE;
				if (opt_name || opt_set)
					goto synopsis;
				opt_name = optarg;
				opt_remove = 1;
				break;

			case 'B':  /* restore */
				if (mode == SET_MODE)
					goto synopsis;
				mode = RESTORE_MODE;
				opt_restore++;
				restore_args = realloc(restore_args,
					opt_restore * sizeof(*restore_args));
				if (!restore_args) {
					perror(progname);
					exit(1);
				}
				restore_args[opt_restore - 1] = optarg;
				break;

			case 'P': /* --physical */
				if (mode == SET_MODE)
					goto synopsis;
				mode = RESTORE_MODE;
				opt_physical = 1;
				break;

			case 'V':
				printf("%s " VERSION "\n", progname);
				return 0;

			case 'H':
				help();
				return 0;

			default:
				goto synopsis;
		}
	}
	if (!(((opt_remove || opt_set) && optind < argc) || opt_restore))
		goto synopsis;

	if (mode == RESTORE_MODE) {
		for (opt = 0; opt < opt_restore; opt++)
			restore(restore_args[opt]);
		free(restore_args);
	}

	while (optind < argc) {
		if (mode == RESTORE_MODE)
			goto synopsis;
		mode = SET_MODE;
		do_set(AT_FDCWD, argv[optind], argv[optind], unquote(opt_name),
		       opt_value);
		optind++;
	}

	return (had_errors ? 1 : 0);

synopsis:
	fprintf(stderr, _("Usage: %s %s\n"), progname, CMD_LINE_SPEC1);
	fprintf(stderr, _("       %s %s\n"), progname, CMD_LINE_SPEC2);
	fprintf(stderr, _("       %s %s\n"), progname, CMD_LINE_SPEC3);
	fprintf(stderr, _("Try `%s --help' for more information.\n"), progname);
	return 2;
}

static int do_set(int dirfd, const char *pathname, const char *fullpath,
		  const char *name, const char *value)
{
	int at_flags = opt_deref ? 0 : AT_SYMLINK_NOFOLLOW;
	size_t size = 0;
	int error;

	if (value) {
		size = strlen(value);
		if (!opt_raw)
			value = decode(value, &size);
		if (!value)
			return 1;
	}
	if (opt_set || opt_restore)
		error = setxattrat(dirfd, pathname, at_flags,
				   name, value, size, 0);
	else
		error = removexattrat(dirfd, pathname, at_flags, name);

	if (error < 0) {
		fprintf(stderr, "%s: %s: %s\n",
			progname, xquote(fullpath, "\n\r"), strerror_ea(errno));
		had_errors++;
		return 1;
	}
	return 0;
}

static const char *decode(const char *value, size_t *size)
{
	static char *decoded;
	static size_t decoded_size;

	if (*size == 0)
		return "";
	if (value[0] == '0' && (value[1] == 'x' || value[1] == 'X')) {
		const char *v = value+2, *end = value + *size;
		char *d;

		if (high_water_alloc((void **)&decoded, &decoded_size,
				     *size / 2)) {
			fprintf(stderr, "%s: %s\n",
				progname, strerror_ea(errno));
			had_errors++;
			return NULL;
		}
		d = decoded;
		while (v < end) {
			int d1, d0;

			while (v < end && isspace(*v))
				v++;
			if (v == end)
				break;
			d1 = hex_digit(*v++);
			while (v < end && isspace(*v))
				v++;
			if (v == end) {
		bad_hex_encoding:
				fprintf(stderr, "bad input encoding\n");
				had_errors++;
				return NULL;
			}
			d0 = hex_digit(*v++);
			if (d1 < 0 || d0 < 0)
				goto bad_hex_encoding;
			*d++ = ((d1 << 4) | d0);
		}
		*size = d - decoded;
	} else if (value[0] == '0' && (value[1] == 's' || value[1] == 'S')) {
		const char *v = value+2, *end = value + *size;
		int d0, d1, d2, d3;
		char *d;

		if (high_water_alloc((void **)&decoded, &decoded_size,
				     *size / 4 * 3)) {
			fprintf(stderr, "%s: %s\n",
				progname, strerror_ea(errno));
			had_errors++;
			return NULL;
		}
		d = decoded;
		for(;;) {
			while (v < end && isspace(*v))
				v++;
			if (v == end) {
				d0 = d1 = d2 = d3 = -2;
				break;
			}
			if (v + 4 > end) {
		bad_base64_encoding:
				fprintf(stderr, "bad input encoding\n");
				had_errors++;
				return NULL;
			}
			d0 = base64_digit(*v++);
			d1 = base64_digit(*v++);
			d2 = base64_digit(*v++);
			d3 = base64_digit(*v++);
			if (d0 < 0 || d1 < 0 || d2 < 0 || d3 < 0)
				break;

			*d++ = (char)((d0 << 2) | (d1 >> 4));
			*d++ = (char)((d1 << 4) | (d2 >> 2));
			*d++ = (char)((d2 << 6) | d3);
		}
		if (d0 == -2) {
			if (d1 != -2 || d2 != -2 || d3 != -2)
				goto bad_base64_encoding;
			goto base64_end;
		}
		if (d0 == -1 || d1 < 0 || d2 == -1 || d3 == -1)
			goto bad_base64_encoding;
		*d++ = (char)((d0 << 2) | (d1 >> 4));
		if (d2 != -2)
			*d++ = (char)((d1 << 4) | (d2 >> 2));
		else {
			if (d1 & 0x0F || d3 != -2)
				goto bad_base64_encoding;
			goto base64_end;
		}
		if (d3 != -2)
			*d++ = (char)((d2 << 6) | d3);
		else if (d2 & 0x03)
			goto bad_base64_encoding;
	base64_end:
		while (v < end && isspace(*v))
			v++;
		if (v + 4 <= end && *v == '=') {
			if (*++v != '=' || *++v != '=' || *++v != '=')
				goto bad_base64_encoding;
			v++;
		}
		while (v < end && isspace(*v))
			v++;
		if (v < end)
			goto bad_base64_encoding;
		*size = d - decoded;
	} else {
		const char *v = value, *end = value + *size;
		char *d;

		if (end > v+1 && *v == '"' && *(end-1) == '"') {
			v++;
			end--;
		}

		if (high_water_alloc((void **)&decoded, &decoded_size, *size)) {
			fprintf(stderr, "%s: %s\n",
				progname, strerror_ea(errno));
			had_errors++;
			return NULL;
		}
		d = decoded;

		while (v < end) {
			if (v[0] == '\\') {
				if (v[1] == '\\' || v[1] == '"') {
					*d++ = *++v; v++;
				} else if (v[1] >= '0' && v[1] <= '7') {
					int c = 0;
					v++;
					c = (*v++ - '0');
					if (*v >= '0' && *v <= '7')
						c = (c << 3) + (*v++ - '0');
					if (*v >= '0' && *v <= '7')
						c = (c << 3) + (*v++ - '0');
					*d++ = c;
				} else
					*d++ = *v++;
			} else
				*d++ = *v++;
		}
		*size = d - decoded;
	}
	return decoded;
}

static int hex_digit(char c)
{
	if (c >= '0' && c <= '9')
		return c - '0';
	else if (c >= 'A' && c <= 'F')
		return c - 'A' + 10;
	else if (c >= 'a' && c <= 'f')
		return c - 'a' + 10;
	else
		return -1;
}

static int base64_digit(char c)
{
	if (c >= 'A' && c <= 'Z')
		return c - 'A';
	else if (c >= 'a' && c <= 'z')
		return 26 + c - 'a';
	else if (c >= '0' && c <= '9')
		return 52 + c - '0';
	else if (c == '+')
		return 62;
	else if (c == '/')
		return 63;
	else if (c == '=')
		return -2;
	else
		return -1;
}

