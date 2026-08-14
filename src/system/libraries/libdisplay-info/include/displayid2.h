#ifndef DISPLAYID2_H
#define DISPLAYID2_H

/**
 * Private header for the low-level DisplayID v2 API.
 */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <libdisplay-info/displayid2.h>

#include "log.h"

struct di_displayid2 {
	int revision;
	enum di_displayid2_product_primary_use_case product_primary_use_case;

	struct di_logger *logger;
};

bool
_di_displayid2_parse(struct di_displayid2 *displayid2, const uint8_t *data,
		     size_t size, struct di_logger *logger);

#endif
