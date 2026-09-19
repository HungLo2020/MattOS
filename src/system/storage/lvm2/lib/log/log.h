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

#ifndef LVM_LOG_H
#define LVM_LOG_H

/*
 * printf()-style macros to use for messages:
 *
 *   log_error   - always print to stderr.
 *   log_print   - always print to stdout.  Use this instead of printf.
 *   log_verbose - print to stdout if verbose is set (-v)
 *   log_very_verbose - print to stdout if verbose is set twice (-vv)
 *   log_debug   - print to stdout if verbose is set three times (-vvv)
 *
 * In addition, messages will be logged to file or syslog if they
 * are more serious than the log level specified with the log/debug_level
 * parameter in the configuration file.  These messages get the file
 * and line number prepended.  'stack' (without arguments) can be used
 * to log this information at debug level.
 *
 * log_sys_error and log_sys_very_verbose are for errors from system calls
 * e.g. log_sys_error("stat", filename);
 *      /dev/fd/7: stat failed: No such file or directory
 *
 */

#include <errno.h>
#include <string.h>

#define EUNCLASSIFIED -1	/* Generic error code */

#define _LOG_FATAL         0x0002
#define _LOG_ERR           0x0003
#define _LOG_WARN          0x0004
#define _LOG_NOTICE        0x0005
#define _LOG_INFO          0x0006
#define _LOG_DEBUG         0x0007
#define _LOG_STDERR        0x0080 /* force things to go to stderr, even if loglevel would make them go to stdout */
#define _LOG_ONCE          0x0100 /* downgrade to NOTICE if this has been already logged */
#define _LOG_BYPASS_REPORT 0x0200 /* do not log through report even if report available */
#define log_level(x)  ((x) & 0x07)			/* obtain message level */
#define log_stderr(x)  ((x) & _LOG_STDERR)		/* obtain stderr bit */
#define log_once(x)  ((x) & _LOG_ONCE)			/* obtain once bit */
#define log_bypass_report(x)  ((x) & _LOG_BYPASS_REPORT)/* obtain bypass bit */

#define INTERNAL_ERROR "Internal error: "

#define LOG_DEBUG_FIELD_ALL		0x0000
#define LOG_DEBUG_FIELD_TIME		0x0001
#define LOG_DEBUG_FIELD_COMMAND		0x0002
#define LOG_DEBUG_FIELD_FILELINE	0x0004
#define LOG_DEBUG_FIELD_MESSAGE		0x0008

#define LOG_JOURNAL_COMMAND             0x0001
#define LOG_JOURNAL_OUTPUT              0x0002
#define LOG_JOURNAL_DEBUG               0x0004


/*
 * Classes available for debug log messages.
 * These are also listed in doc/example.conf
 * and lib/commands/toolcontext.c:_parse_debug_classes()
 */
#define LOG_CLASS_MEM		0x0001	/* "memory" */
#define LOG_CLASS_DEVS		0x0002	/* "devices" */
#define LOG_CLASS_ACTIVATION	0x0004	/* "activation" */
#define LOG_CLASS_ALLOC		0x0008	/* "allocation" */
#define LOG_CLASS_METADATA	0x0020	/* "metadata" */
#define LOG_CLASS_CACHE		0x0040	/* "cache" */
#define LOG_CLASS_LOCKING	0x0080	/* "locking" */
#define LOG_CLASS_LVMPOLLD	0x0100	/* "lvmpolld" */
#define LOG_CLASS_DBUS		0x0200	/* "dbus" */
#define LOG_CLASS_IO		0x0400	/* "io" */

#define log_debug(...) LOG_LINE(_LOG_DEBUG, ##__VA_ARGS__)
#define log_debug_mem(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_MEM, ##__VA_ARGS__)
#define log_debug_devs(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_DEVS, ##__VA_ARGS__)
#define log_debug_activation(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_ACTIVATION, ##__VA_ARGS__)
#define log_debug_alloc(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_ALLOC, ##__VA_ARGS__)
#define log_debug_metadata(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_METADATA, ##__VA_ARGS__)
#define log_debug_cache(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_CACHE, ##__VA_ARGS__)
#define log_debug_locking(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_LOCKING, ##__VA_ARGS__)
#define log_debug_lvmpolld(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_LVMPOLLD, ##__VA_ARGS__)
#define log_debug_dbus(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_DBUS, ##__VA_ARGS__)
#define log_debug_io(...) LOG_LINE_WITH_CLASS(_LOG_DEBUG, LOG_CLASS_IO, ##__VA_ARGS__)

#define log_info(...) LOG_LINE(_LOG_INFO, ##__VA_ARGS__)
#define log_notice(...) LOG_LINE(_LOG_NOTICE, ##__VA_ARGS__)
#define log_warn(...) LOG_LINE(_LOG_WARN | _LOG_STDERR, ##__VA_ARGS__)
#define log_warn_suppress(s, ...) LOG_LINE((s) ? _LOG_NOTICE : (_LOG_WARN | _LOG_STDERR), ##__VA_ARGS__)
#define log_err(...) LOG_LINE_WITH_ERRNO(_LOG_ERR, EUNCLASSIFIED, ##__VA_ARGS__)
#define log_err_suppress(s, ...) LOG_LINE_WITH_ERRNO((s) ? _LOG_NOTICE : _LOG_ERR, EUNCLASSIFIED, ##__VA_ARGS__)
/* First occurrence: error; subsequent occurrences: downgraded to NOTICE (invisible without -v) */
#define log_err_once(...) LOG_LINE_WITH_ERRNO(_LOG_ERR | _LOG_ONCE, EUNCLASSIFIED, ##__VA_ARGS__)
#define log_fatal(...) LOG_LINE_WITH_ERRNO(_LOG_FATAL, EUNCLASSIFIED, ##__VA_ARGS__)

#define stack log_debug("<backtrace>")	/* Backtrace on error */
#define log_very_verbose(...) log_info(__VA_ARGS__)
#define log_verbose(...) log_notice(__VA_ARGS__)
#define log_print(...) LOG_LINE(_LOG_WARN, ##__VA_ARGS__)
#define log_print_unless_silent(...) LOG_LINE(silent_mode() ? _LOG_NOTICE : _LOG_WARN, ##__VA_ARGS__)
#define log_error(...) log_err(__VA_ARGS__)
#define log_error_suppress(s, ...) log_err_suppress(s, __VA_ARGS__)
#define log_error_once(...) log_err_once(__VA_ARGS__)
#define log_errno(...) LOG_LINE_WITH_ERRNO(_LOG_ERR, ##__VA_ARGS__)

/* System call equivalents */
#define log_sys_error(x, y) \
		log_err("%s%s%s failed: %s", y, *y ? ": " : "", x, strerror(errno))
#define log_sys_error_suppress(s, x, y) \
		log_err_suppress(s, "%s%s%s failed: %s", y, *y ? ": " : "", x, strerror(errno))
#define log_sys_very_verbose(x, y) \
		log_info("%s%s%s failed: %s", y, *y ? ": " : "", x, strerror(errno))
#define log_sys_debug(x, y) \
		log_debug("%s%s%s failed: %s", y, *y ? ": " : "", x, strerror(errno))

#define return_0	do { stack; return 0; } while (0)
#define return_NULL	do { stack; return NULL; } while (0)
#define return_false	do { stack; return false; } while (0)
#define return_EINVALID_CMD_LINE \
			do { stack; return EINVALID_CMD_LINE; } while (0)
#define return_ECMD_FAILED do { stack; return ECMD_FAILED; } while (0)
#define goto_out	do { stack; goto out; } while (0)
#define goto_bad	do { stack; goto bad; } while (0)

#endif
