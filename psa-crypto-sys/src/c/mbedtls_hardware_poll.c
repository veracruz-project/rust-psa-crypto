/*
 * AUTHORS
 *
 * The Veracruz Development Team.
 *
 * COPYRIGHT
 *
 * See the `LICENSE_MIT.markdown` file in the Veracruz root directory
 * for licensing and copyright information.
 *
 */

#include "psa/crypto.h"

#if 0
// This is what one would normally do to get the prototype for getrandom,
// but it does not work with version 1.1.19-1 of musl-tools, which comes
// with Ubuntu 18.04 and is currently used for the Nitro build of Veracruz.
#include <sys/random.h>
#else
// This currently works for all targets.
#include <sys/types.h>
ssize_t getrandom(void *buf, size_t buflen, unsigned int flags);
#endif

int mbedtls_hardware_poll(void *data,
                          unsigned char *output, size_t len, size_t *olen)
{
    (void)data;
    ssize_t ret = getrandom(output, len, 0);
    if (ret == -1)
        return PSA_ERROR_GENERIC_ERROR;
    *olen = ret;
    return 0;
}
