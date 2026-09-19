/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2022 Benjamin Berg <bberg@redhat.com>
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

#include "up-daemon.h"
#include "up-enumerator.h"
#include "up-device.h"

typedef struct {
	UpDaemon *daemon;
} UpEnumeratorPrivate;

static void up_enumerator_initable_iface_init (GInitableIface *iface);

G_DEFINE_TYPE_EXTENDED (UpEnumerator, up_enumerator, G_TYPE_OBJECT,
                        G_TYPE_FLAG_ABSTRACT,
                        G_IMPLEMENT_INTERFACE (G_TYPE_INITABLE,
                                               up_enumerator_initable_iface_init)
                        G_ADD_PRIVATE (UpEnumerator))

enum {
  PROP_0,
  PROP_DAEMON,
  N_PROPS
};

static GParamSpec *properties[N_PROPS];

enum {
	SIGNAL_DEVICE_ADDED,
	SIGNAL_DEVICE_REMOVED,
	SIGNAL_LAST
};

static guint signals [SIGNAL_LAST] = { 0 };

UpDaemon*
up_enumerator_get_daemon (UpEnumerator *self)
{
	UpEnumeratorPrivate *priv = up_enumerator_get_instance_private (self);

	return priv->daemon;
}

static void
up_enumerator_init (UpEnumerator *self)
{
}

static gboolean
up_enumerator_initable_init (GInitable *obj,
			     GCancellable *cancellable,
			     GError **error)
{
	UP_ENUMERATOR_GET_CLASS (obj)->initable_init (UP_ENUMERATOR (obj));

	return TRUE;
}

static void
up_enumerator_initable_iface_init (GInitableIface *iface)
{
  iface->init = up_enumerator_initable_init;
}

static void
up_enumerator_set_property (GObject      *object,
                            guint         prop_id,
                            const GValue *value,
                            GParamSpec   *pspec)
{
	UpEnumerator *self = UP_ENUMERATOR (object);
	UpEnumeratorPrivate *priv = up_enumerator_get_instance_private (self);

	switch (prop_id)
	{
	case PROP_DAEMON:
		priv->daemon = g_value_dup_object (value);
		break;

	default:
		G_OBJECT_WARN_INVALID_PROPERTY_ID (object, prop_id, pspec);
	}
}

static void
up_enumerator_dispose (GObject *object)
{
	UpEnumerator *self = UP_ENUMERATOR (object);
	UpEnumeratorPrivate *priv = up_enumerator_get_instance_private (self);

	g_clear_object (&priv->daemon);
}

static void
up_enumerator_class_init (UpEnumeratorClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);

	object_class->dispose = up_enumerator_dispose;

	object_class->set_property = up_enumerator_set_property;

	properties[PROP_DAEMON] =
		g_param_spec_object ("daemon",
		                     "UpDaemon",
		                     "UpDaemon reference",
		                     UP_TYPE_DAEMON,
		                     G_PARAM_STATIC_STRINGS | G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY);

	signals [SIGNAL_DEVICE_ADDED] =
		g_signal_new ("device-added",
			      G_TYPE_FROM_CLASS (klass), G_SIGNAL_RUN_LAST,
			      0,
			      NULL, NULL, NULL,
			      G_TYPE_NONE, 1, UP_TYPE_DEVICE);
	signals [SIGNAL_DEVICE_REMOVED] =
		g_signal_new ("device-removed",
			      G_TYPE_FROM_CLASS (klass), G_SIGNAL_RUN_LAST,
			      0,
			      NULL, NULL, NULL,
			      G_TYPE_NONE, 1, UP_TYPE_DEVICE);

	g_object_class_install_properties (object_class, N_PROPS, properties);
}
