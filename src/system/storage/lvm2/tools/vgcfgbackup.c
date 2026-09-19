/*
 * Copyright (C) 2001-2004 Sistina Software, Inc. All rights reserved.
 * Copyright (C) 2004-2007 Red Hat, Inc. All rights reserved.
 *
 * This file is part of LVM2.
 *
 * This copyrighted material is made available to anyone wishing to use,
 * modify, copy, or redistribute it subject to the terms and conditions
 * of the GNU Lesser General Public License v.2.1.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with this program; if not, write to the Free Software Foundation,
 * Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

#include "tools.h"

__attribute__((nonnull(1, 2, 3)))
static int _expand_filename(const char *template, const char *vg_name,
			    char *last_filename)
{
	char buf[PATH_MAX];

	if (security_level()) {
		if (dm_strncpy(buf, template, sizeof(buf)) < 0) {
			log_error("Failed to copy filename.");
			return 0;
		}
		goto out;
	}

	/* coverity[non_const_printf_format_string] */
	if (dm_snprintf(buf, sizeof(buf), template, vg_name) < 0) {
		log_error("Error processing filename template %s", template);
		return 0;
	}

	if (*last_filename && !strncmp(last_filename, buf, PATH_MAX)) {
		log_error("VGs must be backed up into different files. "
			  "Use %%s in filename for VG name.");
		return 0;
	}
out:
	if (dm_strncpy(last_filename, buf, PATH_MAX) < 0)
		return 0;

	return 1;
}

static int _vg_backup_single(struct cmd_context *cmd, const char *vg_name,
			     struct volume_group *vg,
			     struct processing_handle *handle)
{
	char *last_filename = (char *)handle->custom_handle;

	if (arg_is_set(cmd, file_ARG)) {
		if (!_expand_filename(arg_value(cmd, file_ARG), vg->name,
				      last_filename))
			return_ECMD_FAILED;

		if (!backup_to_file(last_filename, vg->cmd->cmd_line, vg))
			return_ECMD_FAILED;
	} else {
		if (vg_missing_pv_count(vg)) {
			log_error("No backup taken: specify filename with -f to backup with missing PVs.");
			return ECMD_FAILED;
		}
		if (vg_has_unknown_segments(vg)) {
			log_error("No backup taken: specify filename with -f to backup with unknown segments.");
			return ECMD_FAILED;
		}

		/* just use the normal backup code */
		backup_enable(cmd, 1);	/* force a backup */
		if (!backup(vg))
			return_ECMD_FAILED;
	}

	log_print_unless_silent("Volume group \"%s\" successfully backed up.", vg_name);

	return ECMD_PROCESSED;
}

int vgcfgbackup(struct cmd_context *cmd, int argc, char **argv)
{
	int ret;
	char last_filename[PATH_MAX] = "";
	struct processing_handle *handle = NULL;

	if (!(handle = init_processing_handle(cmd, NULL))) {
		log_error("Failed to initialize processing handle.");
		return ECMD_FAILED;
	}

	handle->custom_handle = last_filename;

	/*
	 * Just set so that we can do the check ourselves above and
	 * report a helpful error message in place of the error message
	 * that would be generated from vg_read.
	 */
	cmd->handles_missing_pvs = 1;
	cmd->handles_unknown_segments = 1;

	ret = process_each_vg(cmd, argc, argv, NULL, NULL, 0, 0,
			      handle, &_vg_backup_single);

	destroy_processing_handle(cmd, handle);
	return ret;
}
