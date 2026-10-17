#include "includes.h"
#include "../librpc/ndr/libndr.h"
#include "ndr_dns_utils.h"

/* don't allow an unlimited number of name components. The string must be less
   than 255, with at least one character between dots, so 128 components is
   plenty.
 */
#define MAX_COMPONENTS 128
#define MAX_COMP_LEN    63


/*
 * Copy a DNS component. If it is "good" as a DNS component, return true,
 * otherwise return false. In either case the whole copy is performed.
 */
static bool dns_component_copy(uint8_t *dest, uint8_t *src, size_t len)
{
	size_t i;
	bool ret = true;
	for (i = 0; i < len; i++) {
		uint8_t c = src[i];
		dest[i] = c;
		if (c == '\0') {
			ret = false;
		} else if (c == '.' && i != len - 1) {
			/*
			 * As a special case, for NBT addresses that are also
			 * usernames, we allow a dot as the last character.
			 *
			 * This will not be treated symmetrically on the push
			 * side unless the component is the last component of
			 * an NBT name. In other cases (DNS, non-final NBT)
			 * the trailing dot would truncate the address on push.
			 */
			ret = false;
		}
	}
	return ret;
}

/*
  pull one component of a dns/nbt string
*/
static enum ndr_err_code ndr_pull_component(struct ndr_pull *ndr,
					    uint8_t *component,
					    size_t *component_len,
					    uint32_t *offset,
					    uint32_t *max_offset,
					    const char *err_name)
{
	uint8_t len;
	unsigned int loops = 0;
	bool comp_is_valid;
	*component_len = 0;
	while (loops < 5) {
		if (*offset >= ndr->data_size) {
			return ndr_pull_error(ndr, NDR_ERR_STRING,
					    "BAD %s NAME component, bad offset",
					    err_name);
		}
		len = ndr->data[*offset];
		if (len == 0) {
			*offset += 1;
			*max_offset = MAX(*max_offset, *offset);
			return NDR_ERR_SUCCESS;
		}
		if ((len & 0xC0) == 0xC0) {
			/* its a label pointer */
			if (1 + *offset >= ndr->data_size) {
				return ndr_pull_error(ndr, NDR_ERR_STRING,
						     "BAD %s NAME component, " \
						     "bad label offset",
						     err_name);
			}
			*max_offset = MAX(*max_offset, *offset + 2);
			*offset = ((len&0x3F)<<8) | ndr->data[1 + *offset];
			*max_offset = MAX(*max_offset, *offset);
			loops++;
			continue;
		}
		if ((len & 0xC0) != 0) {
			/* its a reserved length field */
			return ndr_pull_error(ndr, NDR_ERR_STRING,
					      "BAD %s NAME component, " \
					      "reserved length field: 0x%02x",
					      err_name, (len &0xC));
		}
		if (*offset + len + 1 > ndr->data_size ||
		    len > MAX_COMP_LEN /* impossible!, but we live in fear */ ) {
			return ndr_pull_error(ndr, NDR_ERR_STRING,
					      "BAD %s NAME component, "\
					      "length too long",
					      err_name);
		}

		comp_is_valid = dns_component_copy(component,
						   &ndr->data[1 + *offset],
						   len);
		if (! comp_is_valid) {
			return ndr_pull_error(ndr, NDR_ERR_STRING,
					      "BAD %s NAME component, "\
					      "undesirable characters",
					      err_name);
		}
		*component_len = len;
		*offset += len + 1;
		*max_offset = MAX(*max_offset, *offset);
		return NDR_ERR_SUCCESS;
	}

	/* too many pointers */
	return ndr_pull_error(ndr, NDR_ERR_STRING,
			      "BAD %s NAME component, too many pointers",
			      err_name);
}

/**
  pull a dns/nbt string from the wire
*/
enum ndr_err_code ndr_pull_dns_string_list(struct ndr_pull *ndr,
					   ndr_flags_type ndr_flags,
					   const char **s,
					   bool is_nbt)
{
	uint32_t offset = ndr->offset;
	uint32_t max_offset = offset;
	unsigned num_components;
	char *name;
	size_t name_len;
	const char *err_name = NULL;
	size_t max_len;

	/*
	 * maximum *wire* size is 255, per RFC1035, but we compare to 253
	 * because a) there is one more length field than there are separating
	 * dots, and b) there is a final zero-length root node, not
	 * represented in dotted form.
	 *
	 * For NBT we compare to 272 because that is roughly what Windows
	 * 2012r2 does (contra RFC 1002).
	 */
	if (is_nbt) {
		err_name = "NBT";
		max_len = 272;
	} else {
		err_name = "DNS";
		max_len = 253;
	}

	if (!(ndr_flags & NDR_SCALARS)) {
		return NDR_ERR_SUCCESS;
	}

	name = talloc_array(ndr->current_mem_ctx, char, max_len + 2);
	NDR_ERR_HAVE_NO_MEMORY(name);
	name_len = 0;

	for (num_components = 0;
	     num_components < MAX_COMPONENTS;
	     num_components++) {
		uint8_t component[64] = {0, };
		size_t component_len = 0;
		NDR_CHECK(ndr_pull_component(ndr,
					     component,
					     &component_len,
					     &offset,
					     &max_offset,
					     err_name));
		if (component_len == 0) {
			break;
		}
		if (name_len + component_len + 1 > max_len) {
			return ndr_pull_error(ndr, NDR_ERR_STRING,
					      "BAD %s NAME too long",
					      err_name);
		}
		if (name_len > 0) {
			name[name_len] = '.';
			name_len++;
		}

		memcpy(name + name_len, component, component_len);
		name_len += component_len;
	}

	if (num_components == MAX_COMPONENTS) {
		/* We should never reach this */
		return ndr_pull_error(ndr, NDR_ERR_STRING,
				      "BAD %s NAME too many components",
				      err_name);
	}

	name[name_len] = '\0';
	(*s) = name;
	ndr->offset = max_offset;

	return NDR_ERR_SUCCESS;
}


/**
  push a dns/nbt string list to the wire
*/
enum ndr_err_code ndr_push_dns_string_list(struct ndr_push *ndr,
					   struct ndr_token_list *string_list,
					   ndr_flags_type ndr_flags,
					   const char *s,
					   bool is_nbt)
{
	const char *start = s;
	bool use_compression;
	size_t max_length;
	if (is_nbt) {
		use_compression = true;
		/*
		 * Max length is longer in NBT/Wins, because Windows counts
		 * the semi-decompressed size of the netbios name (16 bytes)
		 * rather than the wire size of 32, which is what you'd expect
		 * if it followed RFC1002 (it uses the short form in
		 * [MS-WINSRA]). In other words the maximum size of the
		 * "scope" is 237, not 221.
		 *
		 * We make the size limit slightly larger than 255 + 16,
		 * because the 237 scope limit is already enforced in the
		 * winsserver code with a specific return value; bailing out
		 * here would muck with that.
		 */
		max_length = 274;
	} else {
		use_compression = !(ndr->flags & LIBNDR_FLAG_NO_COMPRESSION);
		max_length = 255;
	}

	if (!(ndr_flags & NDR_SCALARS)) {
		return NDR_ERR_SUCCESS;
	}

	while (s && *s) {
		enum ndr_err_code ndr_err;
		/*
		 * MAX_COMP_LEN + 1 for the length byte, + 1 extra for the
		 * NBT trailing-dot case where complen is incremented to 64.
		 */
		uint8_t compname[MAX_COMP_LEN + 2] = {0, };
		size_t complen;
		uint32_t offset;

		if (use_compression) {
			/* see if we have pushed the remaining string already,
			 * if so we use a label pointer to this string
			 */
			ndr_err = ndr_token_peek_cmp_fn(string_list,
							s,
							&offset,
							(comparison_fn_t)
								strcmp);
			if (NDR_ERR_CODE_IS_SUCCESS(ndr_err)) {
				uint8_t b[2];

				if (offset > 0x3FFF) {
					return ndr_push_error(ndr, NDR_ERR_STRING,
							      "offset for dns string " \
							      "label pointer " \
							      "%"PRIu32"[%08"PRIX32"] > 0x00003FFF",
							      offset, offset);
				}

				b[0] = 0xC0 | (offset>>8);
				b[1] = (offset & 0xFF);

				return ndr_push_bytes(ndr, b, 2);
			}
		}

		complen = strcspn(s, ".");

		/* the length must fit into 6 bits (i.e. <= 63) */
		if (complen > MAX_COMP_LEN) {
			return ndr_push_error(ndr, NDR_ERR_STRING,
					      "component length %zu > %u",
					      complen,
					      MAX_COMP_LEN);
		}

		if (complen == 0 && s[complen] == '.') {
			return ndr_push_error(ndr, NDR_ERR_STRING,
					      "component length is 0 "
					      "(consecutive dots)");
		}

		if (is_nbt && s[complen] == '.' && s[complen + 1] == '\0') {
			/* nbt names are sometimes usernames, and we need to
			 * keep a trailing dot to ensure it is byte-identical,
			 * (not just semantically identical given DNS
			 * semantics). */
			complen++;
		}

		/* remember the current component + the rest of the string
		 * so it can be reused later
		 */
		if (use_compression) {
			NDR_CHECK(ndr_token_store(ndr, string_list, s,
						  ndr->offset));
		}

		/* push just this component into the blob */
		if (unlikely(complen > MAX_COMP_LEN)) {
			return ndr_push_error(ndr, NDR_ERR_STRING,
					      "component length is %zu, should be < %zu",
					      complen, sizeof(compname));
		}
		compname[0] = complen;
		memcpy(compname + 1, s, complen);
		NDR_CHECK(ndr_push_bytes(ndr, compname, complen + 1));

		s += complen;
		if (*s == '.') {
			s++;
		}
		if (s - start > max_length) {
			return ndr_push_error(ndr, NDR_ERR_STRING,
					      "name > %zu characters long",
					      max_length);
		}
	}

	/* if we reach the end of the string and have pushed the last component
	 * without using a label pointer, we need to terminate the string
	 */
	return ndr_push_bytes(ndr, (const uint8_t *)"", 1);
}
