#include "redirect.h"
#include <mstcpip.h>

#define C5_CONTEXT_TAG 'rD5C'

_IRQL_requires_max_(DISPATCH_LEVEL)
NTSTATUS C5RedirectTcp(
    const void *classifyContext,
    const FWPS_INCOMING_METADATA_VALUES0 *metadata,
    UINT64 filterId,
    const C5_REDIRECT_SESSION *session,
    FWPS_CLASSIFY_OUT0 *classifyOut)
{
    UINT64 classifyHandle = 0;
    FWPS_CONNECT_REQUEST0 *request = NULL;
    C5_REDIRECT_CONTEXT *context = NULL;
    NTSTATUS status;

    if ((classifyOut->rights & FWPS_RIGHT_ACTION_WRITE) == 0) {
        return STATUS_ACCESS_DENIED;
    }
    if (session->redirectHandle == NULL || session->relayPid == 0 ||
        session->relayPort == 0 || session->sessionId == 0) {
        status = STATUS_INVALID_PARAMETER;
        goto block;
    }
    if (FWPS_IS_METADATA_FIELD_PRESENT(metadata, FWPS_METADATA_FIELD_REDIRECT_RECORD_HANDLE)) {
        FWPS_CONNECTION_REDIRECT_STATE state = FwpsQueryConnectionRedirectState0(
            metadata->redirectRecords, session->redirectHandle, NULL);
        if (state == FWPS_CONNECTION_REDIRECTED_BY_SELF ||
            state == FWPS_CONNECTION_PREVIOUSLY_REDIRECTED_BY_SELF) {
            return STATUS_SUCCESS;
        }
        if (state != FWPS_CONNECTION_NOT_REDIRECTED) {
            status = STATUS_NOT_SUPPORTED;
            goto block;
        }
    }

    status = FwpsAcquireClassifyHandle0((void *)classifyContext, 0, &classifyHandle);
    if (!NT_SUCCESS(status)) {
        goto block;
    }
    status = FwpsAcquireWritableLayerDataPointer0(
        classifyHandle, filterId, 0, (void **)&request, classifyOut);
    if (!NT_SUCCESS(status)) {
        FwpsReleaseClassifyHandle0(classifyHandle);
        goto block;
    }

    /* The relay listens on loopback only. Explicit non-loopback binds need a
     * separate zone-safe design before they can be routed through this relay. */
    if ((request->remoteAddressAndPort.ss_family != AF_INET &&
         request->remoteAddressAndPort.ss_family != AF_INET6) ||
        request->localAddressAndPort.ss_family != request->remoteAddressAndPort.ss_family ||
        (!INETADDR_ISANY((const SOCKADDR *)&request->localAddressAndPort) &&
         !INETADDR_ISLOOPBACK((const SOCKADDR *)&request->localAddressAndPort)) ||
        request->localRedirectHandle != NULL || request->localRedirectContext != NULL) {
        status = STATUS_NOT_SUPPORTED;
        goto finalize;
    }

    /* FwpsApplyModifiedLayerData0 takes ownership after assignment below, but
     * its header does not express that transfer to MSVC's analyzer. */
#pragma warning(suppress : 6014)
    context = ExAllocatePool2(POOL_FLAG_NON_PAGED, sizeof(*context), C5_CONTEXT_TAG);
    if (context == NULL) {
        status = STATUS_INSUFFICIENT_RESOURCES;
        goto finalize;
    }
    context->version = 1;
    context->size = sizeof(*context);
    context->sessionId = session->sessionId;
    context->originalDestination = request->remoteAddressAndPort;

    INETADDR_SETLOOPBACK((SOCKADDR *)&request->remoteAddressAndPort);
    INETADDR_SET_PORT((SOCKADDR *)&request->remoteAddressAndPort,
                     RtlUshortByteSwap(session->relayPort));
    request->localRedirectTargetPID = session->relayPid;
    request->localRedirectHandle = session->redirectHandle;
    request->localRedirectContext = context;
    request->localRedirectContextSize = sizeof(*context);
    /* WFP owns this allocation once the modified request is applied. */
    context = NULL;
    status = STATUS_SUCCESS;

finalize:
    /* Every acquired writable request must be finalized, including rejections. */
    FwpsApplyModifiedLayerData0(classifyHandle, request, 0);
    FwpsReleaseClassifyHandle0(classifyHandle);
    if (NT_SUCCESS(status)) {
        classifyOut->actionType = FWP_ACTION_PERMIT;
        classifyOut->rights |= FWPS_RIGHT_ACTION_WRITE;
        return status;
    }

block:
    classifyOut->actionType = FWP_ACTION_BLOCK;
    classifyOut->rights &= ~FWPS_RIGHT_ACTION_WRITE;
    return status;
}
