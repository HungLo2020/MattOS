#ifndef DI_DISPLAYID2_H
#define DI_DISPLAYID2_H

/**
 * libdisplay-info's low-level API for VESA Display Identification Data
 * (DisplayID) version 2.
 *
 * The library implements DisplayID version 2.1, available at:
 * https://vesa.org/vesa-standards/
 */

#include <stdbool.h>
#include <stdint.h>

/**
 * DisplayID v2 data structure.
 */
struct di_displayid2;

/**
 * Get the DisplayID v2 revision.
 */
int
di_displayid2_get_revision(const struct di_displayid2 *displayid2);

/**
 * Product primary use case identifier, defined in table 2-3.
 */
enum di_displayid2_product_primary_use_case {
	/* Extension section */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_EXTENSION = 0x00,
	/* Test structure */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_TEST = 0x01,
	/* Generic display */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_GENERIC = 0x02,
	/* Television display */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_TV = 0x03,
	/* Desktop productivity display */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_DESKTOP_PRODUCTIVITY = 0x04,
	/* Desktop gaming display */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_DESKTOP_GAMING = 0x05,
	/* Presentation display */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_PRESENTATION = 0x06,
	/* Head-mounted Virtual Reality (VR) display */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_HMD_VR = 0x07,
	/* Head-mounted Augmented Reality (AR) display */
	DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_HMD_AR = 0x08,
};

/**
 * Get the DisplayID v2 product primary use case.
 */
enum di_displayid2_product_primary_use_case
di_displayid2_get_product_primary_use_case(const struct di_displayid2 *displayid2);

#endif
