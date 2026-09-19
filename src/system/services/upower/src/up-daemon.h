/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2008 David Zeuthen <davidz@redhat.com>
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin St, Fifth Floor, Boston, MA  02110-1301  USA
 *
 */

#ifndef __UP_DAEMON_H__
#define __UP_DAEMON_H__

#include <dbus/up-daemon-generated.h>

#include "up-types.h"
#include "up-device-list.h"

G_BEGIN_DECLS

#define UP_TYPE_DAEMON		(up_daemon_get_type ())
#define UP_DAEMON(o)		(G_TYPE_CHECK_INSTANCE_CAST ((o), UP_TYPE_DAEMON, UpDaemon))
#define UP_DAEMON_CLASS(k)	(G_TYPE_CHECK_CLASS_CAST((k), UP_TYPE_DAEMON, UpDaemonClass))
#define UP_IS_DAEMON(o)	(G_TYPE_CHECK_INSTANCE_TYPE ((o), UP_TYPE_DAEMON))
#define UP_IS_DAEMON_CLASS(k)  (G_TYPE_CHECK_CLASS_TYPE ((k), UP_TYPE_DAEMON))
#define UP_DAEMON_GET_CLASS(o) (G_TYPE_INSTANCE_GET_CLASS ((o), UP_TYPE_DAEMON, UpDaemonClass))

typedef struct UpDaemonPrivate UpDaemonPrivate;

typedef struct
{
	UpExportedDaemonSkeleton parent;
	UpDaemonPrivate	*priv;
} UpDaemon;

typedef struct
{
	UpExportedDaemonSkeletonClass parent_class;
} UpDaemonClass;

typedef enum
{
	UP_DAEMON_ERROR_GENERAL,
	UP_DAEMON_ERROR_NOT_SUPPORTED,
	UP_DAEMON_ERROR_NO_SUCH_DEVICE,
	UP_DAEMON_NUM_ERRORS
} UpDaemonError;

#define UP_DAEMON_ERROR up_daemon_error_quark ()

GQuark		 up_daemon_error_quark		(void);
GType		 up_daemon_get_type		(void);
UpDaemon	*up_daemon_new			(void);

/* private */
guint		 up_daemon_get_number_devices_of_type (UpDaemon	*daemon,
						 UpDeviceKind		 type);
UpDeviceList	*up_daemon_get_device_list	(UpDaemon		*daemon);
gboolean	 up_daemon_startup		(UpDaemon		*daemon,
						 GDBusConnection 	*connection);
void		 up_daemon_shutdown		(UpDaemon		*daemon);
void		 up_daemon_set_lid_is_closed	(UpDaemon		*daemon,
						 gboolean		 lid_is_closed);
void		 up_daemon_set_lid_is_present	(UpDaemon		*daemon,
						 gboolean		 lid_is_present);
void		 up_daemon_set_on_battery	(UpDaemon		*daemon,
						 gboolean		 on_battery);
void		 up_daemon_set_warning_level	(UpDaemon		*daemon,
						 UpDeviceLevel		 warning_level);
UpDeviceLevel	 up_daemon_compute_warning_level(UpDaemon		*daemon,
						 UpDeviceState		 state,
						 UpDeviceKind		 kind,
						 gboolean		 power_supply,
						 gdouble		 percentage,
						 gint64			 time_to_empty);
gboolean	 up_daemon_kbd_backlight_composite_update (UpDaemon	*daemon,
							   gboolean composite_signal);
const gchar	*up_daemon_get_charge_icon	(UpDaemon		*daemon,
						 gdouble		 percentage,
						 UpDeviceLevel		 battery_level,
						 gboolean		 charging);
const gchar	*up_daemon_get_state_dir_env_override (UpDaemon *daemon);
gboolean	 up_daemon_polkit_is_allowed	(UpDaemon		*daemon,
						 const gchar		*action_id,
						 GDBusMethodInvocation	*invocation);

void             up_daemon_pause_poll           (UpDaemon               *daemon);
void             up_daemon_resume_poll          (UpDaemon               *daemon);
void		 up_daemon_set_debug		(UpDaemon		*daemon,
						 gboolean		 debug);
gboolean	 up_daemon_get_debug		(UpDaemon		*daemon);

G_END_DECLS

#endif /* __UP_DAEMON_H__ */
