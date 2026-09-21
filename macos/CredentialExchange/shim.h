// The C ABI of macos/CredentialExchange/shim.swift, for build.rs and the Rust caller.
#ifndef NC_CREDENTIAL_EXCHANGE_H
#define NC_CREDENTIAL_EXCHANGE_H
// Returns a JSON array of rows as an owned C string, or NULL with *error_out set.
char *nc_credential_exchange_import(const char *token, char **error_out);
void nc_credential_exchange_free(char *pointer);
#endif
