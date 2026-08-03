/*
  File: walk_tree.h

  Copyright (C) 2007-2026 Andreas Gruenbacher <a.gruenbacher@computer.org>

  This library is free software; you can redistribute it and/or
  modify it under the terms of the GNU Lesser General Public
  License as published by the Free Software Foundation; either
  version 2.1 of the License, or (at your option) any later version.

  This library is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
  Lesser General Public License for more details.

  You should have received a copy of the GNU Lesser General Public
  License along with this library; if not, see <https://www.gnu.org/licenses/>.
*/

#ifndef __WALK_TREE_H
#define __WALK_TREE_H

enum walk_flags {
	WALK_TREE_RECURSIVE = 0x01,
	WALK_TREE_PHYSICAL = 0x02,
	WALK_TREE_LOGICAL = 0x04,
	WALK_TREE_ONE_FILESYSTEM = 0x08,

	WALK_TREE_TOPLEVEL = 0x100,
	WALK_TREE_FAILED = 0x200,
};

struct stat;

extern int walk_tree(const char *pathname, enum walk_flags walk_flags,
		     int (*func)(int, const char *, const char *,
				 unsigned char, enum walk_flags, void *),
		     void *arg);

#endif
