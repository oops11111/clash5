#pragma once

#include <ntddk.h>
#include <fwpsk.h>

typedef struct C5_REDIRECT_CONTEXT {
    UINT32 version;
    UINT32 size;
    UINT64 sessionId;
    SOCKADDR_STORAGE originalDestination;
} C5_REDIRECT_CONTEXT;

typedef struct C5_REDIRECT_SESSION {
    HANDLE redirectHandle;
    UINT64 sessionId;
    UINT32 relayPid;
    UINT16 relayPort;
} C5_REDIRECT_SESSION;

/* Call only for verified target TCP traffic at an ALE connect-redirect layer.
 * Initialize the action to CONTINUE and hold session rundown protection. */
_IRQL_requires_max_(DISPATCH_LEVEL)
NTSTATUS C5RedirectTcp(
    const void *classifyContext,
    const FWPS_INCOMING_METADATA_VALUES0 *metadata,
    UINT64 filterId,
    const C5_REDIRECT_SESSION *session,
    FWPS_CLASSIFY_OUT0 *classifyOut);
