/* getversion.c - Return the npth version (source included)
 * Copyright (C) 2024 g10 Code GmbH
 *
 * This file is part of nPth.
 *
 * nPth is free software; you can redistribute it and/or modify
 * it under the terms of the GNU Lesser General Public License as
 * published by the Free Software Foundation; either version 2.1 of
 * the License, or (at your option) any later version.
 *
 * nPth is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See
 * the GNU Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this program; if not, see <https://www.gnu.org/licenses/>.
 */

#ifndef BUILD_COMMITID
#error This file must be included from the C source
#endif

static const char *
cright_blurb (void)
{
  static const char blurb[] =
    "\n\n"
    "This is nPth " PACKAGE_VERSION " - The New GNU Portable Threads Library\n"
    CRIGHTBLURB
    "\n"
    "("  BUILD_COMMITID " " BUILD_TIMESTAMP ")\n"
    "\n\n";
  return blurb;
}

/* Return the version of the libarry as a string.  DUMMY is only used
 * to be similar to the other libraries version function. */
const char *
npth_get_version (const char *dummy)
{
  if (dummy && dummy[0] == 1 && dummy[1] == 1)
    return cright_blurb ();
  else if (dummy && dummy[0] == 1 && dummy[1] == 2)
    return BUILD_COMMITID;
  return PACKAGE_VERSION;
}
