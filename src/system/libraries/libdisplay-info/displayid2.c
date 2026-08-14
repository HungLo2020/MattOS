#include <errno.h>

#include "bits.h"
#include "displayid.h"
#include "displayid2.h"

/**
 * The size of the mandatory fields in a DisplayID v2 section.
 */
#define DISPLAYID2_MIN_SIZE 5
/**
 * The maximum size of a DisplayID v2 section.
 */
#define DISPLAYID2_MAX_SIZE 256

static bool
validate_checksum(const uint8_t *data, size_t size)
{
	uint8_t sum = 0;
	size_t i;

	for (i = 0; i < size; i++) {
		sum += data[i];
	}

	return sum == 0;
}

bool
_di_displayid2_parse(struct di_displayid2 *displayid2, const uint8_t *data,
		     size_t size, struct di_logger *logger)
{
	size_t section_size;
	int version;
	uint8_t product_primary_use_case;

	if (size < DISPLAYID2_MIN_SIZE) {
		errno = EINVAL;
		return false;
	}

	displayid2->logger = logger;

	version = _di_displayid_parse_version(data, size);
	displayid2->revision = get_bit_range(data[0x00], 3, 0);
	if (version != 2) {
		errno = ENOTSUP;
		return false;
	}

	section_size = (size_t) data[0x01] + DISPLAYID2_MIN_SIZE;
	if (section_size > DISPLAYID2_MAX_SIZE || section_size > size) {
		errno = EINVAL;
		return false;
	}

	if (!validate_checksum(data, section_size)) {
		errno = EINVAL;
		return false;
	}

	product_primary_use_case = data[0x02];
	switch (product_primary_use_case) {
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_EXTENSION:
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_TEST:
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_GENERIC:
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_TV:
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_DESKTOP_PRODUCTIVITY:
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_DESKTOP_GAMING:
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_PRESENTATION:
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_HMD_VR:
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_HMD_AR:
		displayid2->product_primary_use_case = product_primary_use_case;
		break;
	default:
		errno = EINVAL;
		return false;
	}

	displayid2->logger = NULL;
	return true;
}

int
di_displayid2_get_revision(const struct di_displayid2 *displayid2)
{
	return displayid2->revision;
}

enum di_displayid2_product_primary_use_case
di_displayid2_get_product_primary_use_case(const struct di_displayid2 *displayid2)
{
	return displayid2->product_primary_use_case;
}
