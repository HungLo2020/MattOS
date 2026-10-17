/*
 * Unix SMB/CIFS implementation.
 * Samba utility functions
 *
 * Copyright (C) Gary Lockyer 2026
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

#ifndef __LIB_UTIL_ALIGNMENT_H__
#define __LIB_UTIL_ALIGNMENT_H__

#include "replace.h"

/*
* Is the address of the data pointed to aligned on the specified alignment?
*
* @param ptr       The pointer to check
* @param alignment The expected alignment
*
* @return True if pointer has the specified alignment
*         False otherwise
*/
static inline bool is_aligned(const void *ptr, const size_t alignment)
{
	return ((uintptr_t) ptr % alignment)  == 0;
}


/*
* Is the alignment of pointer correct for type
*
* @param ptr  The pointer to check
* @param type The type to check
*
* @return True if pointer is correctly aligned for type
*         False otherwise
*/
#define check_alignment(ptr, type) is_aligned(ptr, _Alignof(type))

#endif
