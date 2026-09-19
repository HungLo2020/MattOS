/*
 * Copyright (C) 2003-2004 Sistina Software, Inc. All rights reserved.
 * Copyright (C) 2004-2025 Red Hat, Inc. All rights reserved.
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

#include "pvmove_poll.h"

static int _is_pvmove_image_removable(struct logical_volume *mimage_lv,
				      void *baton)
{
	uint32_t mimage_to_remove = baton ? *((uint32_t *)baton) : UINT32_MAX;
	struct lv_segment *mirror_seg;

	if (!(mirror_seg = get_only_segment_using_this_lv(mimage_lv))) {
		log_error(INTERNAL_ERROR "%s is not a proper mirror image",
			  mimage_lv->name);
		return 0;
	}

	if (seg_type(mirror_seg, 0) != AREA_LV) {
		log_error(INTERNAL_ERROR "%s is not a pvmove mirror of LV-type",
			  mirror_seg->lv->name);
		return 0;
	}

	if (mimage_to_remove >= mirror_seg->area_count) {
		log_error(INTERNAL_ERROR "Mirror image %" PRIu32 " not found in segment",
			  mimage_to_remove);
		return 0;
	}

	if (seg_lv(mirror_seg, mimage_to_remove) == mimage_lv)
		return 1;

	return 0;
}

static int _detach_pvmove_mirror(struct cmd_context *cmd,
				 struct logical_volume *lv_mirr,
				 int keep_source)
{
	uint32_t mimage_to_remove = 0;

	if (keep_source &&
	    (seg_type(first_seg(lv_mirr), 0) == AREA_LV))
		mimage_to_remove = 1; /* remove the second mirror leg */

	if (!lv_remove_mirrors(cmd, lv_mirr, 1, 0, _is_pvmove_image_removable,
			       &mimage_to_remove, PVMOVE))
		return_0;

	return 1;
}

/* Remove pvmove LV from metadata and commit; shared by abort and finish paths */
static int _remove_pvmove_lv(struct cmd_context *cmd, struct volume_group *vg,
			      struct logical_volume *lv_mirr)
{
	if (!lv_remove(lv_mirr)) {
		log_error("ABORTING: Removal of temporary pvmove LV %s failed.",
			  display_lvname(lv_mirr));
		return 0;
	}

	if (!vg_write(vg) || !vg_commit(vg)) {
		log_error("ABORTING: Failed to write metadata to disk.");
		return 0;
	}

	lockd_lvremove_done(cmd, lv_mirr, NULL, 0);

	if (vg->needs_lockd_free_lvs)
		lockd_free_removed_lvs(cmd, vg, 1);

	/* Mark lockd cleanup done so callers (e.g. _lockd_pvmove_undo) skip it */
	lv_mirr->lock_args = NULL;

	return 1;
}

/*
 * Called to advance the mirror to successive sections of it.
 * (Not called first time or after the last section completes.)
 */
int pvmove_update_metadata(struct cmd_context *cmd, struct volume_group *vg,
			   struct logical_volume *lv_mirr,
			   struct dm_list *lvs_changed __attribute__((unused)),
			   unsigned flags __attribute__((unused)))
{
	if (!lv_update_and_reload(lv_mirr))
		return_0;

	return 1;
}

int pvmove_abort_initial(struct cmd_context *cmd, struct volume_group *vg,
			 struct logical_volume *lv_mirr,
			 struct dm_list *lvs_changed)
{
	/* Deactivate pvmove LV if partially activated */
	if (lv_is_active(lv_mirr) && !deactivate_lv(cmd, lv_mirr))
		log_warn("WARNING: Failed to deactivate %s.", display_lvname(lv_mirr));

	if (!dm_list_empty(lvs_changed)) {
		if (!_detach_pvmove_mirror(cmd, lv_mirr, 1)) {
			log_error("ABORTING: Removal of temporary pvmove mirror %s failed.",
				  display_lvname(lv_mirr));
			return 0;
		}

		if (!lv_is_error(lv_mirr)) {
			log_error(INTERNAL_ERROR "Failed to replace %s with error segment.",
				  display_lvname(lv_mirr));
			return 0;
		}
	}

	if (!_remove_pvmove_lv(cmd, vg, lv_mirr))
		return_0;

	/* Reload participant LVs back to original PV mappings */
	if (!refresh_pvmoved_lvs(lvs_changed))
		log_warn("WARNING: Failed to refresh LVs after pvmove revert.");

	return 1;
}

int pvmove_finish(struct cmd_context *cmd, struct volume_group *vg,
		  struct logical_volume *lv_mirr, struct dm_list *lvs_changed)
{
	uint32_t visible = vg_visible_lvs(lv_mirr->vg);
	struct lv_list *lvl;
	struct lvinfo info;

	if (!dm_list_empty(lvs_changed)) {
		if (!_detach_pvmove_mirror(cmd, lv_mirr, arg_count(cmd, abort_ARG))) {
			/* abort keeps source */
			log_error("ABORTING: Removal of temporary pvmove mirror %s failed.",
				  display_lvname(lv_mirr));
			return 0;
		}

		if (!lv_is_error(lv_mirr)) {
			log_error(INTERNAL_ERROR "ABORTING: Failed to replace %s with error segment.",
				  display_lvname(lv_mirr));
			return 0;
		}
	}

	if (!lv_update_and_reload(lv_mirr))
		return_0;
	/*
	 * Process all LVs that were changed during pvmove.
	 *
	 * First pass: refresh ALL LVs to ensure they are properly resumed.
	 * Warn on failure but continue -- metadata is already committed
	 * and pvmove0 must be cleaned up.  Affected LVs can be refreshed
	 * manually with lvchange --refresh.
	 */
	if (!refresh_pvmoved_lvs(lvs_changed))
		log_warn("WARNING: Failed to refresh LVs after pvmove.");

	sync_local_dev_names(cmd);

	/*
	 * Second pass: Deactivate component LVs that were activated specifically
	 * for pvmove and are no longer needed.
	 */
	dm_list_iterate_items(lvl, lvs_changed) {
		/* Deactivate invisible component LVs that are not in use (open_count == 0).
		 * These were activated specifically for pvmove and are no longer needed. */
		if (!lv_is_visible(lvl->lv) && lv_is_component(lvl->lv)) {
			if (!lv_info(cmd, lvl->lv, 0, &info, 1, 0))
				log_debug_activation("  Cannot get info for %s, skipping deactivation.",
						     display_lvname(lvl->lv));
			else if (!info.exists)
				log_debug_activation("  Component %s already inactive.",
						     display_lvname(lvl->lv));
			else if (info.open_count != 0)
				log_debug_activation("  Component %s still in use (open_count=%u), not deactivating.",
						     display_lvname(lvl->lv), info.open_count);
			else {
				log_debug_activation("  Deactivating unused component: %s",
						     display_lvname(lvl->lv));
				if (!deactivate_lv(cmd, lvl->lv))
					log_warn("WARNING: Failed to deactivate component %s.",
						 display_lvname(lvl->lv));
			}
		}
	}

	sync_local_dev_names(cmd);

	/* Deactivate mirror LV */
	if (!deactivate_lv(cmd, lv_mirr)) {
		log_error("ABORTING: Unable to deactivate temporary volume %s.",
			  display_lvname(lv_mirr));
		return 0;
	}

	/*
	 * LV locks are held throughout pvmove -- no re-acquisition
	 * needed.  Removing pvmove0 releases its cluster lock via
	 * lockd_lvremove_done().  Participant LV locks remain held
	 * for normal operation after LOCKED is cleared.
	 */

	log_verbose("Removing temporary pvmove LV.");
	if (!_remove_pvmove_lv(cmd, vg, lv_mirr))
		return_0;

	/* Sanity check: visible LV count must have decreased after removal;
	 * an increase means cleanup failed and orphaned temp LVs remain. */
	if (visible < vg_visible_lvs(vg)) {
		log_error("ABORTING: Failed to remove temporary logical volume(s).");
		log_print_unless_silent("Please remove orphan temporary logical volume(s) when possible.");
		return 0;
	}

	return 1;
}
