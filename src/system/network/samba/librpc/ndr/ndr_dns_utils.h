
#ifndef _LIBRPC_NDR_NDR_DNS_UTILS_H
#define _LIBRPC_NDR_NDR_DNS_UTILS_H

enum ndr_err_code ndr_pull_dns_string_list(struct ndr_pull *ndr,
					   ndr_flags_type ndr_flags,
					   const char **s,
					   bool is_nbt);

enum ndr_err_code ndr_push_dns_string_list(struct ndr_push *ndr,
					   struct ndr_token_list *string_list,
					   ndr_flags_type ndr_flags,
					   const char *s,
					   bool is_nbt);

#endif
