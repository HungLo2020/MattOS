# PVMOVE Design Document

## How PVMOVE Works

PVMOVE relocates physical extents from one PV to another while LVs remain
accessible.  The mechanism uses a dm-mirror layer inserted under the LV:

```
Before:
  vg-lv
    `-- PV1(PE0..9) | PV2(PE0..4) | PV3(PE0..2)

During pvmove of PV2 -> PV4:
  vg-lv
    |-- identity(PV1)
    |-- mirror(PV2 -> PV4)   <- dm-mirror copies + intercepts writes
    `-- identity(PV3)

After:
  vg-lv
    `-- PV1 | PV4 | PV3
```

Writes to the moving region are intercepted by dm-mirror which ensures
both source and destination are updated.  When copy completes, the mirror
is collapsed and metadata updated to point to the new PV.

### Key Source Files

| File | Role |
|------|------|
| `tools/pvmove.c` | Command entry, LV selection, mirror setup |
| `tools/pvmove_poll.c` | Polling completion, metadata updates, cleanup |
| `tools/polldaemon.c` | Generic polling loop (shared with lvconvert) |
| `lib/metadata/lv_manip.c` | Segment insertion, layer creation, chunking, RAID PV exclusion |
| `lib/metadata/mirror.c` | Mirror manipulation, `refresh_pvmoved_lvs()` |
| `lib/activate/activate.c` | Activation with `track_pvmove_deps` control |
| `lib/activate/dev_manager.c` | Device tree building, pvmove dependency tracking |
| `lib/metadata/lv.c` | `lv_lock_holder()`, `lv_active_change()` pvmove pre-activation |

## Current Implementation

### Operation and Lock Flow

`_pvmove_setup_single()` dispatches to `_pvmove_resume()` or
`_pvmove_create()`.  Lock management is handled within each path:
`_lockd_pvmove_new()` acquires, `_lockd_pvmove_undo()` releases on error.

```
SETUP  (pvmove <pv> [-n <lv>])
  pvmove()
    [lvmlockd] require lvmpolld; require args (except --abort)
    [--abort] set lockd_vg_default_sh
  process_each_pv(..., READ_FOR_UPDATE)
    VG EX lock acquired (lvmlockd for shared VGs, file lock otherwise)
    _pvmove_setup_single()
      [named LV (-n)]
        _extract_lvname()
      [resume path: find_pvmove_lv() finds existing pvmove0]
        _pvmove_resume()                         -- see RESUME below
      [new pvmove path]
        _pvmove_create()                         -- see CREATE below
    VG lock released on return

CREATE  (_pvmove_create)
  [named LV] find_lv(vg, lv_name) -> lv_move
  source_pvl = create_pv_list()
  allocatable_pvs = _get_allocatable_pvs()
  _find_moving_lvs()                             -- find striped LVs on source PV
  [empty -> error "No data to move"]
  _skip_unmovable_lvs()                          -- remove locked/unmoveable
    [named LV + unmoveable -> error]
    [unnamed + unmoveable -> warn + skip]
  [shared VG]
    _skip_remote_lvs()                           -- probe remote lock state
      _is_lockd_locally_held(lv_top)             -- shared probe logic
        active: lockd_query_lv() verify EX held
        inactive: lockd_lv("ex") probe, lockd_lv("un") release
      [named LV] failure -> error
      [unnamed]  failure -> warn + skip
  [empty after skips -> error "All data on source PV skipped"]
  lv_mirr = _create_pvmove_lv()                 -- lv_create_empty("pvmove%d")
  _populate_pvmove_lv()                          -- insert mirrors + finalize
    _insert_pvmove_mirrors() per LV              -- set LOCKED on participants
    _finalize_pvmove_lv()                        -- convert to mirrored + split
      lv_add_mirrors(MIRROR_BY_SEG)
        build_parallel_areas_from_lv()           -- per-segment RAID PV exclusion
        allocate_extents(parallel_areas)
  _copy_id_components()                          -- save id for polling
  [shared VG]
    _lockd_pvmove_new()                          -- init lock args + EX on pvmove0
  _update_metadata_and_activate()
    find first active LV in changed_lvs
    [all inactive] vg_write + vg_commit          -- metadata only
    [active LV found] lv_update_and_reload(lv)   -- suspend/resume
    activate_lv(lv_mirr)                         -- start data copy
    [activation failure]
      pvmove_abort_initial()                     -- revert metadata
      _lockd_pvmove_undo()                       -- release pvmove0 lock

POLLING  (lvmpolld, one call per completed section)
  pvmove_update_metadata()
    lv_update_and_reload(lv_mirr)                -- advance to next section

FINISH  (last section complete or --abort)
  pvmove_finish()
    _detach_pvmove_mirror()                      -- remove mirror leg
    lv_update_and_reload(lv_mirr)                -- replace with error target
    refresh_pvmoved_lvs()                        -- suspend+resume moved LVs
    deactivate component LVs (non-fatal)         -- cleanup invisible components
    deactivate_lv(lv_mirr)                       -- deactivate pvmove mirror
    _remove_pvmove_lv()
      lv_remove(lv_mirr)
      vg_write/vg_commit
      lockd_lvremove_done()                      -- unlock pvmove0
      lockd_free_removed_lvs()                   -- free pvmove0 lock space
      lv_mirr->lock_args = NULL                  -- mark lockd cleanup done

RESUME  (pvmove <pv> when pvmove already in progress)
  _pvmove_resume()
    _copy_id_components()                        -- save id for polling
    _pvmove_validate_resume()                    -- find changed_lvs, check
                                                 -- --name match, warn ignored args
    [shared VG]
      lockd_lv(lv_mirr, "ex", PERSISTENT)        -- re-acquire EX on pvmove0
    activate_lv(lv_mirr)                         -- resume data copy
    [activation failure + shared VG]
      lockd_lv(lv_mirr, "un")                    -- release pvmove0 lock

ABORT  (pvmove --abort, including initial activation failure)
  pvmove_abort_initial()
    deactivate_lv(lv_mirr)                       -- if partially activated
    _detach_pvmove_mirror(keep_source=1)         -- keep original PV data
    _remove_pvmove_lv()                          -- remove pvmove0 + lockd cleanup
    refresh_pvmoved_lvs()                        -- reload original mappings
```

### Helper Functions in pvmove.c

| Function | Purpose |
|----------|---------|
| `_pvmove_setup_single()` | Entry: parse lv_name, dispatch to resume or create |
| `_pvmove_create()` | New pvmove orchestrator: find, filter, create, populate, lock, activate |
| `_pvmove_resume()` | Resume orchestrator: validate, re-acquire lock, activate |
| `_pvmove_validate_resume()` | Resume validation: find changed_lvs, name match check |
| `_find_moving_lvs()` | Find striped LVs on source PV (pure discovery, no filtering) |
| `_skip_unmovable_lvs()` | Filter locked/unmoveable LVs from moving_lvs |
| `_skip_remote_lvs()` | Filter remotely-locked LVs via lockd_lv probe (shared VG) |
| `_is_lockd_locally_held()` | Probe cluster lock state: query for active, try-acquire for inactive |
| `_create_pvmove_lv()` | Create empty pvmove LV with PVMOVE\|LOCKED flags |
| `_populate_pvmove_lv()` | Insert mirrors for each moving LV, call finalize |
| `_finalize_pvmove_lv()` | Check empty mirror, add mirror legs, split parent segments |
| `_update_metadata_and_activate()` | Write metadata, reload active LVs, activate pvmove LV |
| `_lv_is_pvmoveable()` | Check if LV can be pvmoved (locked, writecache, integrity, sanlock) |
| `_lockd_pvmove_new()` | Initialize lock args + acquire EX on pvmove LV |
| `_lockd_pvmove_undo()` | Release pvmove LV lock + free lock space (skips if lock_args cleared) |
| `_copy_id_components()` | Clone VG/LV name + LVID for polling |

### Segment Chunking

Large segments are split before mirroring via `pvmove_max_segment_size_mb`
configuration (`lib/metadata/lv_manip.c`).  This prevents mirroring
excessively large amounts of data in a single operation and provides
better progress granularity.

```
allocation {
    pvmove_max_segment_size_mb = 10240  # 10 GiB chunks
}
```

### RAID PV Exclusion During Allocation

When pvmove moves a RAID sub-LV, the mirror allocation must avoid
placing the new copy on a PV that holds a sibling sub-LV of the same
RAID array (otherwise redundancy is broken).

This is handled per-segment inside the allocator via `parallel_areas`,
built by `build_parallel_areas_from_lv()` (`lib/metadata/lv_manip.c`).
Each pvmove segment links back to its source sub-LV through
`pvmove_source_seg`.  `_add_raid_exclusion_pvs()` uses that link to
find the RAID parent and adds PVs of all sibling sub-LVs to that
segment's avoidance list.  Same-index partners (rmeta<->rimage) are
skipped to allow collocation.

Because `parallel_areas` is per-segment, each RAID LV gets its own
exclusion constraint.  This avoids the problem of global PV exclusion,
where one RAID LV's constraint could block allocation for an unrelated
RAID LV that shares the same source PV.

### Lock Holder Resolution

`lv_lock_holder()` (`lib/metadata/lv.c`) recursively finds the LV
that holds the lock.  Special cases:
- COW snapshots -> follow to origin
- Thin pools -> pool is the lock holder.  In clustered VGs, only
  the pool holds a cluster lock; thin volumes share it.
- pvmove0 -> follows `pvmove_source_seg` back to the participating LV

### DM Suspend Ordering

`lv_update_and_reload()` suspends the LV's full dependency tree before
loading new tables, then resumes.  This is required for atomic table
transitions so in-flight I/O sees consistent targets.

### track_pvmove_deps

`track_pvmove_deps` in `struct dev_manager` controls whether the
device-tree builder walks `segs_using_this_lv` upward from pvmove0 to
discover all participating LVs.

- **Activation/suspend**: `track_pvmove_deps = 1` -- full walk needed
  to load mirror tables for all participants atomically.
- **Deactivation**: `track_pvmove_deps = 0` -- upward walk suppressed.
  Kernel deps walk provides the correct dependency hierarchy at level 1,
  where open_count > 0 safely prevents premature deactivation of pvmove0.

### Inactive LV Handling

pvmove inserts mirror segments in metadata for ALL affected LVs
regardless of their activation state.  Inactive LVs are NOT activated
to install mirrors -- the mirror takes effect when the LV is next
activated.  The pvmove mirror LV (pvmove0) is always activated to
perform the actual data copy.

Key implementation points:

- `_create_pvmove_lv()` creates the pvmove LV; `_remove_pvmove_lv()`
  in `pvmove_poll.c` handles removal on finish/abort.
- `_update_metadata_and_activate()` iterates `changed_lvs` for the
  first active LV to use with `lv_update_and_reload()`.  If no LV is
  active, does `vg_write()` + `vg_commit()` without suspend/resume
  (no kernel tables to reload).
- On initial activation failure of pvmove0, `pvmove_abort_initial()`
  reverts: detach mirror (keep source data), remove pvmove LV, refresh
  participant LVs back to original mappings.

### Completion: suspend+resume for Table Reload

`refresh_pvmoved_lvs()` (`lib/metadata/mirror.c`) uses
`lv_refresh_suspend_resume()` rather than `activate_lv()`.  After
the pvmove layer is removed, component LVs (e.g. `_tmeta`) still
have kernel tables pointing to the old pvmove device.  suspend+resume
forces DM table reload from committed metadata.  Lock holders are
deduplicated so multiple component LVs sharing the same holder do not
trigger redundant refreshes.

### Component LV Deactivation at Finish

After `refresh_pvmoved_lvs()`, `pvmove_finish()` iterates `lvs_changed`
and deactivates invisible component LVs (e.g. `_tmeta`, `_tdata`) that
were activated specifically for pvmove and are no longer needed.
Uses `lv_info()` with `open_count` check: only deactivates components
with `open_count == 0`.  Failures are non-fatal (log_warn).

### Deactivation During Active pvmove

`lvchange -an LV` during active pvmove correctly deactivates only
the requested LV.  pvmove0 and other participating LVs remain active
and pvmove continues.

Two fixes in `lib/activate/activate.c` and `lib/activate/dev_manager.c`:

1. `_lv_deactivate()` sets `track_pvmove_deps = 0` -- prevents the
   upward walk from pvmove0 to all participating LVs via
   `segs_using_this_lv`.

2. In `_add_lv_to_dtree()` segment area loop, pvmove area LVs are
   skipped during DEACTIVATE/CLEAN actions (pvmove LV treated as a
   PV boundary).  The kernel deps walk already places pvmove0 at
   level 1 (child of the deactivating LV), where open_count > 0
   safely prevents deactivation.  ACTIVATE/PRELOAD/SUSPEND still
   do the full segment walk for atomic table reload.

### vgchange -an With Active pvmove

`vgchange -an` checks for active pvmove LVs before starting
deactivation (`tools/vgchange.c`, `vgchange_activate()`):

- **No force flag**: report error, direct user to `pvmove --abort`.
- **-f**: prompt before interrupting pvmove.
- **-ff or -f -y**: warn and interrupt without prompting.

The check runs before the deactivation loop.  If placed after,
deactivating the last visible LV drops pvmove's open count to zero,
allowing `dm_tree_deactivate_children` to silently remove pvmove0.

Interrupted pvmove leaves mirror metadata intact; pvmove can be
restarted from the last completed segment.

### Force-Removal of pvmove-Locked LVs

After `vgchange -ff -an` interrupts a pvmove, metadata contains
LOCKED LVs with mirror segments but no active DM devices.
`lvremove -f` (DONT_PROMPT) removes them with a warning.

`lv_remove_single()` (`lib/metadata/lv_manip.c`):

1. Detects the pvmove LV the LOCKED participant references
   (AREA_LV with PVMOVE flag) before removing the participant.
2. After `lv_remove()` frees the participant's segments (removing
   its reference from `pvmove_lv->segs_using_this_lv`), checks
   whether pvmove_lv is now unreferenced.
3. If so, calls `lv_remove()` on pvmove_lv to free PV extents.
   Calls `lockd_free_lv_queue()` first for shared VGs.

Works for both MIRROR_BY_SEG (AREA_PV in pvmove segments) and
MIRROR_BY_SEGMENTED_LV (mimage sub-LVs cascade via lv_is_mirror_image).

### Cluster Lock for pvmove LV in Shared VGs

pvmove0 actively modifies data but being a hidden LV it would not
normally acquire a cluster lock.  If all visible LVs are deactivated,
no lock holder would remain and other cluster nodes would be unaware
pvmove is running.

- `lockd_lv_uses_lock()` returns 1 for pvmove LVs (before the
  visibility check) -- pvmove LVs acquire cluster locks.
- `_lockd_pvmove_new()` calls `lockd_init_lv_args()` after pvmove0
  creation, then acquires EX on it.
- `_lockd_pvmove_undo()` releases the pvmove LV lock + frees lock
  space on error.  Skips if `lock_args` is NULL (already cleaned up
  by `_remove_pvmove_lv`).
- Lock space is freed via `lockd_lvremove_done()` +
  `lockd_free_removed_lvs()` in `_remove_pvmove_lv()` (both the
  normal completion and abort paths), and also via
  `lockd_free_lv_queue()` in the force-removal path
  (`lv_remove_single()`).

### LV Lock Lifecycle During pvmove

In shared VGs, pvmove holds only the pvmove0 EX lock persistently.
Participant LV locks are NOT held by pvmove itself.  Protection
against remote activation relies on the LOCKED metadata flag and the
`_lv_pvmove_is_active()` guard in `lockd_lv()`.

1. **Setup** (`_pvmove_create`): `_skip_remote_lvs()` probes each
   participant's lock state but does NOT keep any LV lock held:
   - Active LVs: `lockd_query_lv()` verifies local EX is already
     held (via activation).
   - Inactive LVs: `lockd_lv("ex")` probe then immediate
     `lockd_lv("un")` release -- just checks no remote host holds it.
   `_lockd_pvmove_new()` acquires the only persistent lock: EX on
   pvmove0.

2. **During pvmove**: the LOCKED flag set on each participant LV
   blocks remote activation via the `lockd_lv()` guard:
   ```c
   if (lv_is_locked(lv) && !_lv_pvmove_is_active(lv))
       return_0;
   ```
   - On the pvmove node: pvmove0 is active, so
     `_lv_pvmove_is_active()` returns 1 and `lockd_lv()` proceeds
     normally -- activation/deactivation of LOCKED LVs works.
   - On a remote node: pvmove0 is NOT active locally, so
     `_lv_pvmove_is_active()` returns 0 and `lockd_lv()` fails --
     activation is refused regardless of cluster lock state.

3. **Finish** (`_remove_pvmove_lv`): pvmove0 lock is released via
   `lockd_lvremove_done()` and lock space freed via
   `lockd_free_removed_lvs()`.  LOCKED flag is cleared from
   participant LVs when mirror is detached.

4. **Resume** (explicit `pvmove <pv>` after crash): `_pvmove_resume()`
   re-acquires EX on pvmove0 and activates it.  Participant LV locks
   are not re-acquired (protection is via LOCKED flag + pvmove0
   activity, not per-LV locks).

5. **Restart** (implicit via `vgchange -aey` after reboot):
   `lv_active_change()` detects LOCKED LVs with inactive pvmove0
   and pre-activates the pvmove LV (acquiring its lock) before
   calling `lockd_lv()` on the participant.  This path does not
   resume polling -- it only restores activation so `pvmove <pv>`
   can be run afterwards to resume the data copy.

### LOCKED LV Activation Guard

`lockd_lv()` (`lib/locking/lvmlockd.c`) handles LOCKED LVs through
the `_lv_pvmove_is_active()` check.  Three cases:

- **On pvmove node** (pvmove0 active): `_lv_pvmove_is_active()`
  returns 1, `lockd_lv()` proceeds normally.  Lock/unlock operations
  work as usual.  Active participants keep their own EX lock (from
  activation).
- **On restart node** (pvmove0 not active, no lock held -- after
  reboot): `lv_active_change()` (`lib/metadata/lv.c`) pre-activates
  pvmove0 before calling `lockd_lv()`.  This resolves the chicken-
  and-egg: pvmove0 is invisible so `vgchange -aey` never activates
  it directly, but `lockd_lv()` requires pvmove0 active to allow
  LOCKED LV activation.  The pre-activation acquires the pvmove0
  lock (providing multi-host protection) and makes
  `_lv_pvmove_is_active()` return 1 for the subsequent `lockd_lv()`
  call.  The `!lv_is_pvmove(lv)` guard prevents infinite recursion
  since pvmove LVs also carry the LOCKED flag.
- **On remote node** (pvmove0 not active, lock held by pvmove host):
  pre-activation of pvmove0 in `lv_active_change()` fails at lock
  acquisition (pvmove host holds EX), so `_lv_pvmove_is_active()`
  returns 0 and `lockd_lv()` returns 0.  Activation is refused.

This mechanism is metadata-based (LOCKED flag in VG metadata +
pvmove0 local activity check), not cluster-lock-based.  It works
regardless of whether the participant LV holds its own cluster lock.

### Why lockd_query_lv Is Not Used in the pvmove Path

An alternative approach would be `lockd_query_lv()` to check whether a
remote host held a lock on an LV before deciding to skip it during
pvmove.  Direct `lockd_lv(EX)` lock attempts are used instead for two
reasons:

1. **Lock managers do not reliably support state queries.**
   DLM in particular has no "query who holds this lock" operation.
   `lockd_query_lv()` worked in lvmlockd's test mode and with sanlock
   (which stores lock state on disk), but is not a portable primitive
   across lock backends.  The correct pattern is to *try to acquire*
   the lock -- failure means another host holds it.

2. **Try-lock is race-free.**
   A query returns a snapshot that can go stale before the caller acts
   on it (TOCTOU).  A lock attempt either succeeds (caller now holds
   it) or fails (someone else does), with no window in between.  The
   VG EX lock (READ_FOR_UPDATE) already serializes metadata changes,
   so the lock attempt under VG EX is definitive.

`lockd_query_lv()` IS used for active LVs in `_skip_remote_lvs()`
where the lock is already held by the local activation -- querying
is sufficient since the lock state cannot change while the LV is
active locally.  `lockd_query_lv()` is also used by
`lib/report/report.c` for displaying lock status (best-effort).

### Test Lock Injection (lvmlockd --test mode)

`lvmlockctl --set-remote-lv-lock <vg> --lv-uuid <uuid> --lock-mode <mode>`
injects remote lock state into lvmlockd's daemon_test mode for testing
without real DLM/sanlock infrastructure.

Sets `r->test_remote_ex` or `r->test_remote_sh` flags on the resource
without creating any `struct lock` entries.  Simulates another cluster
node holding a lock.  The `lm_lock_*()` functions check these flags
when `daemon_test` is set:
- No remote flags: lock succeeds (return 0)
- SH request + remote SH only: compatible (return 0)
- All other combinations: return -EAGAIN

The resource is kept alive across client disconnect so the injected
state persists until explicitly cleared with `--lock-mode un`.

The operation is rejected unless lvmlockd runs in `--test` mode.

## Resolved Design Questions

1. **Completion for LVs that stayed inactive throughout pvmove**:
   pvmove0 copies ALL data regardless of participant activation state.
   `_detach_pvmove_mirror()` collapses mirrors in metadata for all
   participants.  Inactive LVs pick up the new PV mapping on next
   activation.

2. **LV activated during pvmove poll loop**:
   Activation reads committed metadata (mirror segments already present),
   loading DM tables that reference pvmove0.  At finish, `lvs_using_lv()`
   queries metadata to find ALL participants regardless of when they were
   activated.  `refresh_pvmoved_lvs()` does suspend+resume only for
   active LVs.  No special mid-pvmove tracking needed.

3. **Progress reporting when some LVs are inactive**:
   `poll_mirror_progress()` queries pvmove0's DM mirror which copies ALL
   extents (active + inactive LVs).  Progress percentage already reflects
   total work.

4. **Interaction with dmeventd monitoring**:
   pvmove segments are explicitly skipped for dmeventd registration
   (`seg->status & PVMOVE` check in `monitor_dev_for_events()`).  pvmove
   uses its own polling mechanism (lvmpolld / polldaemon).  dmeventd is
   completely uninvolved.

## Implementation Plan

### Phase 1: Skip Forced Activation -- DONE

1. Insert mirror segments in metadata for all affected LVs -- unchanged.
2. Remove force-activation of inactive LVs -- done.
3. Remove separate `activate_lvs()` call -- done.
4. Fix `_update_metadata_and_activate()` for first-LV-inactive case --
   done: iterates `changed_lvs` for first active LV.
5. Remove exclusive handling -- done.

### Phase 2: Deactivation During pvmove -- DONE

1. Fix `_lv_deactivate()` to not track pvmove deps -- done.
2. Fix deactivation tree: treat pvmove LV as PV boundary in segment
   walk -- done.
3. Fix `refresh_pvmoved_lvs()`: replace `activate_lv()` with
   `lv_refresh_suspend_resume()` -- done.
4. vgchange -an with active pvmove: error + force flags -- done.
5. Force-removal of pvmove-locked LVs after interruption -- done.

### Phase 3: Cluster Lock for pvmove LV -- DONE

1. `lockd_lv_uses_lock()` returns 1 for pvmove LVs -- done.
2. `_lockd_pvmove_new()` / `_lockd_pvmove_undo()` for pvmove0 lock
   lifecycle -- done.
3. Lock space freed on pvmove LV removal via `_remove_pvmove_lv()` in
   `pvmove_poll.c` -- done.
4. `_skip_remote_lvs()`: probe-and-release per-LV lock to detect remote
   activity; `lockd_query_lv()` for active LVs -- done.
5. Resume path re-acquires pvmove0 lock only -- done.
6. `_pvmove_setup_single()` dispatches to `_pvmove_resume()` or
   `_pvmove_create()` -- done.
7. Refactored helpers: `_find_moving_lvs()` (pure discovery),
   `_skip_unmovable_lvs()` / `_skip_remote_lvs()` (filtering),
   `_is_lockd_locally_held()` (cluster lock probe),
   `_create_pvmove_lv()`, `_populate_pvmove_lv()`,
   `_finalize_pvmove_lv()`,
   `_update_metadata_and_activate()` -- done.
8. RAID PV exclusion moved from global `allocatable_pvs` filtering
   to per-segment avoidance via `_add_raid_exclusion_pvs()` in
   `build_parallel_areas_from_lv()` (lv_manip.c) -- done.
9. LOCKED LV activation guard (`_lv_pvmove_is_active` in `lockd_lv`) -- done.
9b. Activation ordering: `lv_active_change()` pre-activates pvmove0
    before `lockd_lv()` for LOCKED LVs (resolves restart-after-reboot
    chicken-and-egg with `vgchange -aey`) -- done.
10. lvmlockd test lock injection: --set-remote-lv-lock -- done.
11. Double lockd cleanup fix: `_remove_pvmove_lv` clears `lock_args`
    after lockd cleanup; `_lockd_pvmove_undo` checks `lock_args` before
    cleanup -- done.
12. Verification with lvmlockd --test environment -- done
   (`test/shell/lvmlockd-pvmove-locks.sh`).

### Phase 4: Remove track_pvmove_deps parameter -- open

Remove `track_pvmove_deps` parameter from `dev_manager_create()`.
All call sites use `!lv_is_pvmove(lv)` -- the tree builder can check
this internally instead of requiring callers to pass it.

### Phase 5: Polling Lock Optimization -- open

SH->EX lock upgrade in polling loop.

### Phase 6: Clustered VG Support -- open

Add pre-check: abort if any participating LV is active on another
node but not locally.  Uses `lockd_lv()` / lock state to detect
remote activation.  pvmove must run on the node where LVs are active.

## Test Coverage

`test/shell/pvmove-inactive.sh`:

Tests 1-4 use `-n` and run in both shared (lvmlockd) and non-shared configs.
Tests 5-8 move all LVs from a PV without naming them.

- Test 1: pvmove -n moves only named inactive LV; other LVs not touched
- Test 2: pvmove abort with background daemon (-i +3 polling)
- Test 3: Deactivate unrelated LV while pvmove of named LV is in progress
- Test 4: vgchange -an fails with pvmove; vgchange -ff -an
          interrupts and deactivates
- Test 5: Mix of active and inactive LVs on source PV
- Test 6: All participating LVs inactive
- Test 7: Inactive LV first in segment order
- Test 8: Inactive thin-pool sub-LV on source PV
- Test 8b: Active thin-pool pvmove (component LV deactivation)
- Test 8c: RAID1 pvmove (per-segment RAID PV exclusion coverage)
- Test 9: Abort with mixed active/inactive LVs
- Test 10: Concurrent pvmove skips locked LVs
- Test 11: Resume preserves inactive state
- Test 12: pvmove refuses to move writecache cachevol PV
- Test 13: Named pvmove of locked LV fails with error message
- Test 14: All LVs locked on source PV -- empty mirror, "skipped" error
- Test 15: RAID sub-LV named pvmove (sibling PV exclusion coverage)
- Test 16: Inactive RAID holder verbose message

`test/shell/lvmlockd-pvmove-locks.sh` (requires LVM_TEST_LVMLOCKD_TEST):

- Test 1: lvmlockctl --set-remote-lv-lock round-trip: inject EX/UN modes,
          verify remote lock blocks activation
- Test 2: Pre-activation of pvmove LV on restart: pvmove0 removed from
          DM, lv_active_change() pre-activates it, activation succeeds
- Test 2b: LOCKED LV activation refused when pvmove lock held remotely:
           inject remote EX on pvmove0, pre-activation fails at lock
- Test 3: pvmove --abort releases pvmove0 lock space (lockd_lvremove_done
          + lockd_free_removed_lvs path); LV lock remains held
- Test 4: Unnamed pvmove in shared VG: remotely-locked LV skipped, other moved
- Test 4b: Unnamed pvmove fails when ALL LVs remotely locked (empty mirror)
- Test 5: Normal pvmove completion releases pvmove0 lock space; LV lock
          persists for normal operation
- Test 6: Named pvmove refused when remote EX held on named LV; clears
          after lock release and pvmove succeeds
- Test 7: LOCKED LV keeps its own EX lock during pvmove; activation
          succeeds on pvmove node (lock already held); deactivation
          does not release pvmove's lock
- Test 8: LV lock persists through pvmove finish: active participant
          retains its EX lock after pvmove0 is freed
- Test 9: Named pvmove of already-LOCKED LV is refused
- Test 10: Persistent locks survive poll daemon death; abort cleans up
- Test 11: Resume with wrong LV name refused (not part of in-progress pvmove)
- Test 12: Named pvmove succeeds with remotely-locked unrelated LV

`test/shell/pvmove-raid-redundancy.sh`:

- Test 1: RAID1 pvmove refuses to collocate mirror legs
- Test 2: RAID1 pvmove to a free PV preserves redundancy
- Test 3: RAID1 sibling collocation (rmeta follows rimage)
- Test 4: RAID1 rmeta cannot move to the other leg's PV
- Test 5: RAID10 unnamed pvmove preserves stripe and mirror redundancy
- Test 6: RAID10 refuses cross-mirror and cross-stripe collocation
- Test 7: RAID5 pvmove preserves parity distribution
- Test 8: Unnamed pvmove with multiple RAID LVs on same PV
- Test 9: Per-LV RAID PV exclusion with constrained destinations
- Test 10: RAID1 inactive pvmove preserves redundancy

## Open Questions

1. **Phase 4 (track_pvmove_deps cleanup)**: `track_pvmove_deps` is
   passed as a parameter to `dev_manager_create()`.  Every call site
   uses the same pattern: `!lv_is_pvmove(lv)` (i.e. track=1 for normal
   LVs, track=0 when operating on pvmove0 itself).  The tree builder
   could determine this internally instead of requiring callers to pass
   it.  Mechanical refactoring -- remove the parameter from
   `dev_manager_create()` and have `dev_manager` check `lv_is_pvmove()`
   on the LV it receives.
