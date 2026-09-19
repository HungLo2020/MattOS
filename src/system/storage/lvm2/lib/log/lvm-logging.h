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

#ifndef LVM_LOGGING_H
#define LVM_LOGGING_H

#include "lib/misc/lvm-file.h"

#include <stdint.h>

struct dm_report;
struct id;

__attribute__ ((format(printf, 5, 6)))
void print_log(int level, const char *file, int line, int dm_errno_or_class,
	       const char *format, ...);

__attribute__ ((format(printf, 5, 6)))
void print_log_libdm(int level, const char *file, int line, int dm_errno_or_class,
		     const char *format, ...);

#define LOG_LINE(l, ...) \
    print_log(l, __FILE__, __LINE__ , 0, ##__VA_ARGS__)

#define LOG_LINE_WITH_ERRNO(l, e, ...) \
    print_log(l, __FILE__, __LINE__ , e, ##__VA_ARGS__)

#define LOG_LINE_WITH_CLASS(l, c, ...) \
    print_log(l, __FILE__, __LINE__ , c, ##__VA_ARGS__)

#include "lib/log/log.h"

int init_custom_log_streams(struct custom_fds *custom_fds);
int reopen_standard_stream(FILE **stream, const char *mode);

#ifndef LVM2_LOG_FN_T_DEFINED
#define LVM2_LOG_FN_T_DEFINED

typedef void (*lvm2_log_fn_t) (int level, const char *file, int line,
			       int dm_errno_or_class, const char *message);

#endif

void init_log_fn(lvm2_log_fn_t log_fn);

void init_indent(int indent);
void init_msg_prefix(const char *prefix);

void init_debug_file_fields(uint32_t debug_fields);
void init_debug_output_fields(uint32_t debug_fields);

void init_log_file(const char *log_file, int append);
void unlink_log_file(int ret);
void init_log_while_suspended(int log_while_suspended);
void init_abort_on_internal_errors(int fatal);

void fin_log(void);
void reset_log_duplicated(void);

void init_syslog(int enable, int facility);
void fin_syslog(void);

void init_log_journal(uint32_t fields);
uint32_t log_journal_str_to_val(const char *str);

void log_command(const char *cmd_line, const char *cmd_name, const char *cmd_id);


void reset_lvm_errno(int store_errmsg);
int stored_errno(void);
const char *stored_errmsg(void);
const char *stored_errmsg_with_clear(void);

/* Suppress messages to stdout/stderr (1) or everywhere (2) */
/* Returns previous setting */
int log_suppress(int suppress);

/* Suppress messages to syslog */
void syslog_suppress(int suppress);

/* Hooks to handle logging through report. */
typedef enum {
	LOG_REPORT_CONTEXT_NULL,
	LOG_REPORT_CONTEXT_SHELL,
	LOG_REPORT_CONTEXT_PROCESSING,
	LOG_REPORT_CONTEXT_COUNT
} log_report_context_t;

typedef enum {
	LOG_REPORT_OBJECT_TYPE_NULL,
	LOG_REPORT_OBJECT_TYPE_PRE_CMD,
	LOG_REPORT_OBJECT_TYPE_CMD,
	LOG_REPORT_OBJECT_TYPE_ORPHAN,
	LOG_REPORT_OBJECT_TYPE_PV,
	LOG_REPORT_OBJECT_TYPE_LABEL,
	LOG_REPORT_OBJECT_TYPE_VG,
	LOG_REPORT_OBJECT_TYPE_LV,
	LOG_REPORT_OBJECT_TYPE_COUNT
} log_report_object_type_t;

typedef struct log_report {
	struct dm_report *report;
	log_report_context_t context;
	log_report_object_type_t object_type;
	const char *object_name;
	const struct id *object_id;
	const char *object_group;
	const struct id *object_group_id;
} log_report_t;

#define LOG_STATUS_NAME "status"
#define LOG_STATUS_SUCCESS "success"
#define LOG_STATUS_FAILURE "failure"

log_report_t log_get_report_state(void);
void log_restore_report_state(log_report_t log_report);

void log_set_report(struct dm_report *report);
void log_set_report_context(log_report_context_t context);
void log_set_report_object_type(log_report_object_type_t object_type);
void log_set_report_object_group_and_group_id(const char *group, const struct id *group_id);
void log_set_report_object_name_and_id(const char *name, const struct id *id);

const char *log_get_report_context_name(log_report_context_t context);
const char *log_get_report_object_type_name(log_report_object_type_t object_type);

#endif
