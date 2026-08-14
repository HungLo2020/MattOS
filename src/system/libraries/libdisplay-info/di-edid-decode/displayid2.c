#include <stdio.h>
#include <stdlib.h>

#include <libdisplay-info/displayid2.h>

#include "di-edid-decode.h"

static const char *
displayid2_product_primary_use_case_name(enum di_displayid2_product_primary_use_case use_case)
{
	switch (use_case) {
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_EXTENSION:
		return "Same primary use case as the base section";
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_TEST:
		return "Test Structure; test equipment only";
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_GENERIC:
		return "None of the listed primary use cases; generic display";
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_TV:
		return "Television (TV) display";
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_DESKTOP_PRODUCTIVITY:
		return "Desktop productivity display";
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_DESKTOP_GAMING:
		return "Desktop gaming display";
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_PRESENTATION:
		return "Presentation display";
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_HMD_VR:
		return "Head-mounted Virtual Reality (VR) display";
	case DI_DISPLAYID2_PRODUCT_PRIMARY_USE_CASE_HMD_AR:
		return "Head-mounted Augmented Reality (AR) display";
	}
	abort(); /* unreachable */
}

void
print_displayid2(const struct di_displayid2 *displayid2)
{
	enum di_displayid2_product_primary_use_case use_case;

	printf("  Version: 2.%d\n", di_displayid2_get_revision(displayid2));

	use_case = di_displayid2_get_product_primary_use_case(displayid2);
	printf("  Display Product Primary Use Case: %s\n",
	       displayid2_product_primary_use_case_name(use_case));
}
