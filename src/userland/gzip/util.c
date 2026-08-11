/* util.c -- utility functions for gzip support

   Copyright (C) 1997-1999, 2001-2002, 2006, 2009-2025 Free Software
   Foundation, Inc.
   Copyright (C) 1992-1993 Jean-loup Gailly

   This program is free software; you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation; either version 3, or (at your option)
   any later version.

   This program is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.

   You should have received a copy of the GNU General Public License
   along with this program.  If not, see <https://www.gnu.org/licenses/>.  */

#include <config.h>
#include <ctype.h>
#include <errno.h>
#include <limits.h>
#include <unistd.h>
#include <fcntl.h>
#include <stdlib.h>

#include "crc.h"
#include "tailor.h"
#include "gzip.h"
#include <dirname.h>
#include <xalloc.h>

#ifndef EPIPE
# define EPIPE 0
#endif

static int write_buffer (int, voidp, unsigned int);

/* Shift register contents.  */
static ulg crc = 0L;

/* ===========================================================================
 * Copy input to output unchanged: zcat == cat with --force.
 * IN assertion: insize bytes have already been read in inbuf and inptr bytes
 * already processed or copied.
 * 'in' and 'out' are the input and output file descriptors.
 */
int
copy (int in, int out)
{
    int got;

    errno = 0;
    while (insize > inptr) {
        write_buf(out, (char*)inbuf + inptr, insize - inptr);
        got = read_buffer (in, (char *) inbuf, INBUFSIZ);
        if (got == -1)
            read_error();
        bytes_in += got;
        insize = (unsigned)got;
        inptr = 0;
    }
    return OK;
}

/* ===========================================================================
 * Run a set of bytes through the crc shift register.  If s is a NULL
 * pointer, then initialize the crc shift register contents instead.
 * Return the current crc in either case.
 * S points to N bytes to pump through.
 */
ulg
updcrc (uch const *s, unsigned n)
{
    crc = (s == NULL ? 0 : crc32_update (crc, (const char *) s, n));
    return crc;
}

/* Return a current CRC value.  */
ulg
getcrc ()
{
  return crc;
}

#ifdef IBM_Z_DFLTCC
/* Set a new CRC value.  */
void
setcrc (ulg c)
{
  crc = c;
}
#endif

/* ===========================================================================
 * Clear input and output buffers
 */
void clear_bufs()
{
    outcnt = 0;
    insize = inptr = 0;
    bytes_in = bytes_out = 0L;
}

/* ===========================================================================
 * Fill the input buffer. This is called only when the buffer is empty.
 * EOF_OK is set if EOF acceptable as a result.
 */
int
fill_inbuf (int eof_ok)
{
    int len;

    /* Read as much as possible */
    insize = 0;
    do {
        len = read_buffer (ifd, (char *) inbuf + insize, INBUFSIZ - insize);
        if (len == 0) break;
        if (len == -1) {
          read_error();
          break;
        }
        insize += len;
    } while (insize < INBUFSIZ);

    if (insize == 0) {
        if (eof_ok) return EOF;
        flush_window();
        errno = 0;
        read_error();
    }
    bytes_in += (off_t)insize;
    inptr = 1;
    return inbuf[0];
}

/* Like the standard read function, except do not attempt to read more
   than INT_MAX bytes at a time.  */
int
read_buffer (int fd, voidp buf, unsigned int cnt)
{
  int len;
  if (INT_MAX < cnt)
    cnt = INT_MAX;
  len = read (fd, buf, cnt);

#if defined F_SETFL && O_NONBLOCK && defined EAGAIN
  /* Input files are opened O_NONBLOCK for security reasons.  On some
     file systems this can cause read to fail with errno == EAGAIN.  */
  if (len < 0 && errno == EAGAIN)
    {
      int flags = fcntl (fd, F_GETFL);
      if (0 <= flags)
        {
          if (! (flags & O_NONBLOCK))
            errno = EAGAIN;
          else if (fcntl (fd, F_SETFL, flags & ~O_NONBLOCK) != -1)
            len = read (fd, buf, cnt);
        }
    }
#endif

  return len;
}

/* Likewise for 'write'.  */
static int
write_buffer (int fd, voidp buf, unsigned int cnt)
{
  if (INT_MAX < cnt)
    cnt = INT_MAX;
  return write (fd, buf, cnt);
}

/* ===========================================================================
 * Write the output buffer outbuf[0..outcnt-1] and update bytes_out.
 * (used for the compressed data only)
 */
void flush_outbuf()
{
    if (outcnt == 0) return;

    write_buf (ofd, outbuf, outcnt);
    outcnt = 0;
}

/* ===========================================================================
 * Write the output window window[0..outcnt-1] and update crc and bytes_out.
 * (Used for the decompressed data only.)
 */
void flush_window()
{
    if (outcnt == 0) return;
    updcrc(window, outcnt);

    write_buf (ofd, window, outcnt);
    outcnt = 0;
}

/* ===========================================================================
 * Update the count of output bytes.  If testing, do not do any
 * output.  Otherwise, write the buffer, checking for errors.
 */
void
write_buf (int fd, voidp buf, unsigned  cnt)
{
    unsigned  n;

    bytes_out += cnt;
    if (test)
      return;

    while ((n = write_buffer (fd, buf, cnt)) != cnt) {
        if (n == (unsigned)(-1)) {
            write_error();
        }
        cnt -= n;
        buf = (voidp)((char*)buf+n);
    }
}

/* ========================================================================
 * Put string s in lower case, return s.
 */
char *
strlwr (char *s)
{
    char *t;
    for (t = s; *t; t++)
      *t = tolow ((unsigned char) *t);
    return s;
}

/* ========================================================================
 * Return the base name of a file (remove any directory prefix and
 * any version suffix). For systems with file names that are not
 * case sensitive, force the base name to lower case.
 */
char *
gzip_base_name (char *fname)
{
    fname = last_component (fname);
    if (casemap('A') == 'a') strlwr(fname);
    return fname;
}

/* ========================================================================
 * Unlink a file, working around the unlink readonly bug (if present).
 */
int
xunlink (char *filename)
{
  int r = unlink (filename);

#ifdef UNLINK_READONLY_BUG
  if (r != 0)
    {
      int e = errno;
      if (chmod (filename, S_IWUSR) != 0)
        {
          errno = e;
          return -1;
        }

      r = unlink (filename);
    }
#endif

  return r;
}

#ifdef NO_MULTIPLE_DOTS
/* ========================================================================
 * Make a file name legal for file systems not allowing file names with
 * multiple dots or starting with a dot (such as MSDOS), by changing
 * all dots except the last one into underlines.  A target dependent
 * function can be used instead of this simple function by defining the macro
 * MAKE_LEGAL_NAME in tailor.h and providing the function in a target
 * dependent module.
 */
void
make_simple_name (char *name)
{
    char *p = strrchr(name, '.');
    if (p == NULL) return;
    if (p == name) p++;
    do {
        if (*--p == '.') *p = '_';
    } while (p != name);
}
#endif

/* Convert the value of the environment variable ENVVAR_NAME
   to a newly allocated argument vector, and set *ARGCP and *ARGVP
   to the number of arguments and to the vector, respectively.
   Make the new vector's zeroth element equal to the old **ARGVP.
   Return a pointer to the newly allocated string storage.

   If the vector would be empty, do not allocate storage,
   do not set *ARGCP and *ARGVP, and return NULL.  */

#define SEPARATOR	" \t"	/* separators in env variable */

char *add_envopt(
    int *argcp,          /* pointer to argc */
    char ***argvp,       /* pointer to argv */
    char const *envvar_name) /* name of environment variable */
{
    char *p;             /* running pointer through env variable */
    char **oargv;        /* runs through old argv array */
    char **nargv;        /* runs through new argv array */
    int  nargc = 0;      /* number of arguments in env variable */
    char *env_val;

    env_val = getenv(envvar_name);
    if (env_val == NULL) return NULL;

    env_val = xstrdup (env_val);

    for (p = env_val; *p; nargc++ ) {        /* move through env_val */
        p += strspn(p, SEPARATOR);	     /* skip leading separators */
        if (*p == '\0') break;

        p += strcspn(p, SEPARATOR);	     /* find end of word */
        if (*p) *p++ = '\0';		     /* mark it */
    }
    if (nargc == 0) {
        free(env_val);
        return NULL;
    }
    *argcp = nargc + 1;
    /* Allocate the new argv array, with an extra element just in case
     * the original arg list did not end with a NULL.
     */
    nargv = xcalloc (*argcp + 1, sizeof (char *));
    oargv  = *argvp;
    *argvp = nargv;

    /* Copy the program name first */
    *nargv++ = *oargv;

    /* Then copy the environment args */
    for (p = env_val; nargc > 0; nargc--) {
        p += strspn(p, SEPARATOR);	     /* skip separators */
        *(nargv++) = p;			     /* store start */
        while (*p++) ;			     /* skip over word */
    }

    *nargv = NULL;
    return env_val;
}

/* ========================================================================
 * Error handlers.
 */
void
gzip_error (char const *m)
{
    fprintf (stderr, "\n%s: %s: %s\n", program_name, ifname, m);
    abort_gzip();
}

void
xalloc_die ()
{
  fprintf (stderr, "\n%s: memory_exhausted\n", program_name);
  abort_gzip ();
}

void warning (char const *m)
{
    WARN ((stderr, "%s: %s: warning: %s\n", program_name, ifname, m));
}

void read_error()
{
    fprintf (stderr, "\n%s: %s: %s\n",
             program_name, ifname,
             errno ? strerror (errno) : "unexpected end of file");
    abort_gzip();
}

void write_error()
{
  int exitcode = errno == EPIPE ? WARNING : ERROR;
  if (! (exitcode == WARNING && quiet))
    fprintf (stderr, "\n%s: %s: %s\n", program_name, ofname, strerror (errno));
  finish_up_gzip (exitcode);
}

/* ========================================================================
 * Display compression ratio on the given stream on 6 characters.
 */
void
display_ratio (off_t num, off_t den, FILE *file)
{
    fprintf(file, "%5.1f%%", den == 0 ? 0 : 100.0 * num / den);
}
