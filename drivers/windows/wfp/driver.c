#include <ntifs.h>
#include <initguid.h>
#include "driver.h"

#define C5_DEVICE_NAME L"\\Device\\Clash5ProcessProxy"
#define C5_DOS_DEVICE_NAME L"\\DosDevices\\Clash5ProcessProxy"

DEFINE_GUID(C5_WFP_PROVIDER_KEY,
            0x39d1d3b9, 0x609c, 0x4e90, 0x8b, 0x89, 0x0a, 0x14, 0x01, 0x43, 0x94, 0xe3);
DEFINE_GUID(C5_WFP_CALLOUT_V4_KEY,
            0x29c874bb, 0x6e17, 0x4d6a, 0x9e, 0x51, 0x49, 0x21, 0x0c, 0x3b, 0x4e, 0xd4);
DEFINE_GUID(C5_WFP_CALLOUT_V6_KEY,
            0xf5227b0e, 0x0ad0, 0x4b46, 0xae, 0x64, 0x2a, 0x99, 0x20, 0x02, 0xe4, 0x90);
DEFINE_GUID(C5_DEVICE_CLASS_KEY,
            0x2a4b8158, 0xece9, 0x4d21, 0xb6, 0xcd, 0x1f, 0x7b, 0xea, 0xfa, 0x25, 0x72);

static C5_DRIVER_STATE *gC5State;

static BOOLEAN C5SnapshotSessionForProcess(C5_DRIVER_STATE *state, UINT64 processId,
                                            C5_REDIRECT_SESSION *session)
{
    KIRQL oldIrql;
    UINT16 index;
    BOOLEAN found = FALSE;

    KeAcquireSpinLock(&state->sessionLock, &oldIrql);
    if (state->sessionActive) {
        for (index = 0; index < state->targetCount; index++) {
            if (state->targets[index].pid == processId &&
                ExAcquireRundownProtection(&state->sessionRundown)) {
                *session = state->session;
                found = TRUE;
                break;
            }
        }
    }
    KeReleaseSpinLock(&state->sessionLock, oldIrql);
    return found;
}

_IRQL_requires_max_(PASSIVE_LEVEL)
static void NTAPI C5ProcessNotify(HANDLE parentId, HANDLE processId, BOOLEAN create)
{
    C5_DRIVER_STATE *state = gC5State;
    KIRQL oldIrql;
    UINT16 index;

    UNREFERENCED_PARAMETER(parentId);
    UNREFERENCED_PARAMETER(create);
    if (state == NULL) {
        return;
    }
    /* Serialize exit notification with identity validation and publication. */
    ExAcquireFastMutex(&state->processLock);
    KeAcquireSpinLock(&state->sessionLock, &oldIrql);
    if (state->sessionActive) {
        for (index = 0; index < state->targetCount; index++) {
            if (state->targets[index].pid == (UINT64)(ULONG_PTR)processId) {
                RtlZeroMemory(&state->targets[index], sizeof(state->targets[index]));
                break;
            }
        }
    }
    KeReleaseSpinLock(&state->sessionLock, oldIrql);
    ExReleaseFastMutex(&state->processLock);
}

_IRQL_requires_max_(DISPATCH_LEVEL)
static void NTAPI C5ClassifyConnect(
    const FWPS_INCOMING_VALUES0 *fixedValues,
    const FWPS_INCOMING_METADATA_VALUES0 *metadata,
    void *layerData,
    const void *classifyContext,
    const FWPS_FILTER1 *filter,
    UINT64 flowContext,
    FWPS_CLASSIFY_OUT0 *classifyOut)
{
    C5_DRIVER_STATE *state = gC5State;
    C5_REDIRECT_SESSION session;
    UINT32 protocolField;

    UNREFERENCED_PARAMETER(layerData);
    UNREFERENCED_PARAMETER(flowContext);
    if ((classifyOut->rights & FWPS_RIGHT_ACTION_WRITE) == 0) {
        return;
    }
    classifyOut->actionType = FWP_ACTION_CONTINUE;
    if (!FWPS_IS_METADATA_FIELD_PRESENT(metadata, FWPS_METADATA_FIELD_PROCESS_ID) ||
        state == NULL) {
        return;
    }

    if (fixedValues->layerId == FWPS_LAYER_ALE_CONNECT_REDIRECT_V4) {
        protocolField = FWPS_FIELD_ALE_CONNECT_REDIRECT_V4_IP_PROTOCOL;
    } else if (fixedValues->layerId == FWPS_LAYER_ALE_CONNECT_REDIRECT_V6) {
        protocolField = FWPS_FIELD_ALE_CONNECT_REDIRECT_V6_IP_PROTOCOL;
    } else {
        return;
    }
    if (fixedValues->incomingValue[protocolField].value.uint8 != IPPROTO_TCP) {
        return;
    }
    if (!C5SnapshotSessionForProcess(state, metadata->processId, &session)) {
        return;
    }

    (void)C5RedirectTcp(classifyContext, metadata, filter->filterId, &session, classifyOut);
    ExReleaseRundownProtection(&state->sessionRundown);
}

_IRQL_requires_max_(PASSIVE_LEVEL)
static NTSTATUS NTAPI C5Notify(FWPS_CALLOUT_NOTIFY_TYPE notifyType,
                               const GUID *filterKey, FWPS_FILTER1 *filter)
{
    UNREFERENCED_PARAMETER(notifyType);
    UNREFERENCED_PARAMETER(filterKey);
    UNREFERENCED_PARAMETER(filter);
    return STATUS_SUCCESS;
}

static NTSTATUS C5RegisterCallout(PDEVICE_OBJECT deviceObject, const GUID *calloutKey,
                                  UINT32 *calloutId)
{
    FWPS_CALLOUT1 callout;

    RtlZeroMemory(&callout, sizeof(callout));
    callout.calloutKey = *calloutKey;
    callout.classifyFn = C5ClassifyConnect;
    callout.notifyFn = C5Notify;
    return FwpsCalloutRegister1(deviceObject, &callout, calloutId);
}

static void C5ClearSession(C5_DRIVER_STATE *state)
{
    KIRQL oldIrql;

    KeAcquireSpinLock(&state->sessionLock, &oldIrql);
    state->sessionActive = FALSE;
    state->targetCount = 0;
    RtlZeroMemory(state->targets, sizeof(state->targets));
    KeReleaseSpinLock(&state->sessionLock, oldIrql);
    ExWaitForRundownProtectionRelease(&state->sessionRundown);
    RtlZeroMemory(&state->session, sizeof(state->session));
}

static NTSTATUS C5DetachSession(C5_DRIVER_STATE *state, const C5_DETACH_REQUEST *request)
{
    KIRQL oldIrql;
    NTSTATUS status = STATUS_SUCCESS;

    if (request->version != C5_SESSION_PROTOCOL_VERSION || request->size != sizeof(*request) ||
        request->sessionId == 0) {
        return STATUS_INVALID_PARAMETER;
    }
    if (KeGetCurrentIrql() > APC_LEVEL) {
        return STATUS_INVALID_DEVICE_STATE;
    }
    ExAcquireFastMutex(&state->processLock);
    KeAcquireSpinLock(&state->sessionLock, &oldIrql);
    if (!state->sessionActive) {
        /* Recovery may follow a persisted intent that crashed before attach. */
        status = STATUS_SUCCESS;
    } else if (state->session.sessionId != request->sessionId) {
        status = STATUS_ACCESS_DENIED;
    } else {
        state->sessionActive = FALSE;
        state->targetCount = 0;
        RtlZeroMemory(state->targets, sizeof(state->targets));
    }
    KeReleaseSpinLock(&state->sessionLock, oldIrql);
    if (NT_SUCCESS(status)) {
        ExWaitForRundownProtectionRelease(&state->sessionRundown);
        RtlZeroMemory(&state->session, sizeof(state->session));
        ExReInitializeRundownProtection(&state->sessionRundown);
    }
    ExReleaseFastMutex(&state->processLock);
    return status;
}

static NTSTATUS C5AttachSession(C5_DRIVER_STATE *state, const C5_ATTACH_REQUEST *request)
{
    C5_REDIRECT_SESSION session;
    PEPROCESS processes[C5_MAX_TARGET_PIDS];
    KIRQL oldIrql;
    UINT16 index;
    UINT16 candidate;
    UINT16 referencedCount = 0;
    NTSTATUS status = STATUS_SUCCESS;

    if (request->version != C5_SESSION_PROTOCOL_VERSION || request->size != sizeof(*request) ||
        request->sessionId == 0 || request->relayPid == 0 || request->relayPort == 0 ||
        request->targetCount == 0 || request->targetCount > C5_MAX_TARGET_PIDS) {
        return STATUS_INVALID_PARAMETER;
    }
    if (KeGetCurrentIrql() > APC_LEVEL) {
        return STATUS_INVALID_DEVICE_STATE;
    }
    RtlZeroMemory(processes, sizeof(processes));
    for (index = 0; index < request->targetCount; index++) {
        if (request->targets[index].pid == 0 || request->targets[index].createdAt == 0 ||
            request->targets[index].pid == request->relayPid) {
            status = STATUS_INVALID_PARAMETER;
            goto cleanup;
        }
        status = PsLookupProcessByProcessId((HANDLE)(ULONG_PTR)request->targets[index].pid,
                                            &processes[index]);
        if (!NT_SUCCESS(status)) {
            goto cleanup;
        }
        referencedCount++;
        if ((UINT64)PsGetProcessCreateTimeQuadPart(processes[index]) != request->targets[index].createdAt) {
            status = STATUS_INVALID_PARAMETER;
            goto cleanup;
        }
        for (candidate = 0; candidate < index; candidate++) {
            if (request->targets[index].pid == request->targets[candidate].pid) {
                status = STATUS_INVALID_PARAMETER;
                goto cleanup;
            }
        }
    }
    /* Exit callbacks cannot pass this lock between the final liveness check and
     * publication. References keep the queried process objects available. */
    ExAcquireFastMutex(&state->processLock);
    for (index = 0; index < request->targetCount; index++) {
        if (PsGetProcessExitStatus(processes[index]) != STATUS_PENDING) {
            status = STATUS_INVALID_PARAMETER;
            goto unlock;
        }
    }
    RtlZeroMemory(&session, sizeof(session));
    session.redirectHandle = state->redirectHandle;
    session.sessionId = request->sessionId;
    session.relayPid = request->relayPid;
    session.relayPort = request->relayPort;

    KeAcquireSpinLock(&state->sessionLock, &oldIrql);
    if (state->sessionActive) {
        status = STATUS_DEVICE_BUSY;
    } else {
        state->session = session;
        state->targetCount = request->targetCount;
        RtlCopyMemory(state->targets, request->targets,
                      request->targetCount * sizeof(request->targets[0]));
        state->sessionActive = TRUE;
    }
    KeReleaseSpinLock(&state->sessionLock, oldIrql);

unlock:
    ExReleaseFastMutex(&state->processLock);
cleanup:
    for (index = 0; index < referencedCount; index++) {
        ObDereferenceObject(processes[index]);
    }
    return status;
}

_Function_class_(DRIVER_DISPATCH)
static NTSTATUS C5Dispatch(PDEVICE_OBJECT deviceObject, PIRP irp)
{
    C5_DRIVER_STATE *state = (C5_DRIVER_STATE *)deviceObject->DeviceExtension;
    PIO_STACK_LOCATION stack = IoGetCurrentIrpStackLocation(irp);
    NTSTATUS status;

    if (stack->MajorFunction != IRP_MJ_DEVICE_CONTROL) {
        status = STATUS_SUCCESS;
    } else if (stack->Parameters.DeviceIoControl.IoControlCode == IOCTL_C5_ATTACH_SESSION &&
               stack->Parameters.DeviceIoControl.InputBufferLength == sizeof(C5_ATTACH_REQUEST)) {
        status = C5AttachSession(state, (const C5_ATTACH_REQUEST *)irp->AssociatedIrp.SystemBuffer);
    } else if (stack->Parameters.DeviceIoControl.IoControlCode == IOCTL_C5_DETACH_SESSION &&
               stack->Parameters.DeviceIoControl.InputBufferLength == sizeof(C5_DETACH_REQUEST)) {
        const C5_DETACH_REQUEST *request = (const C5_DETACH_REQUEST *)irp->AssociatedIrp.SystemBuffer;
        status = C5DetachSession(state, request);
    } else {
        status = STATUS_INVALID_DEVICE_REQUEST;
    }
    irp->IoStatus.Status = status;
    irp->IoStatus.Information = 0;
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    return status;
}

_Function_class_(DRIVER_UNLOAD)
static void C5Unload(PDRIVER_OBJECT driverObject)
{
    C5_DRIVER_STATE *state = gC5State;
    UNICODE_STRING dosDeviceName = RTL_CONSTANT_STRING(C5_DOS_DEVICE_NAME);

    if (state != NULL) {
        if (state->processNotifyRegistered) {
            PsSetCreateProcessNotifyRoutine(C5ProcessNotify, TRUE);
        }
        C5ClearSession(state);
        if (state->calloutV6Id != 0) {
            FwpsCalloutUnregisterById0(state->calloutV6Id);
        }
        if (state->calloutV4Id != 0) {
            FwpsCalloutUnregisterById0(state->calloutV4Id);
        }
        if (state->redirectHandle != NULL) {
            FwpsRedirectHandleDestroy0(state->redirectHandle);
        }
        gC5State = NULL;
    }
    IoDeleteSymbolicLink(&dosDeviceName);
    if (driverObject->DeviceObject != NULL) {
        IoDeleteDevice(driverObject->DeviceObject);
    }
}

_Function_class_(DRIVER_INITIALIZE)
NTSTATUS DriverEntry(PDRIVER_OBJECT driverObject, PUNICODE_STRING registryPath)
{
    UNICODE_STRING deviceName = RTL_CONSTANT_STRING(C5_DEVICE_NAME);
    UNICODE_STRING dosDeviceName = RTL_CONSTANT_STRING(C5_DOS_DEVICE_NAME);
    PDEVICE_OBJECT deviceObject = NULL;
    C5_DRIVER_STATE *state;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(registryPath);
    status = IoCreateDeviceSecure(driverObject, sizeof(C5_DRIVER_STATE), &deviceName,
                                  FILE_DEVICE_NETWORK, FILE_DEVICE_SECURE_OPEN, FALSE,
                                  &SDDL_DEVOBJ_SYS_ALL_ADM_ALL, &C5_DEVICE_CLASS_KEY,
                                  &deviceObject);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    state = (C5_DRIVER_STATE *)deviceObject->DeviceExtension;
    RtlZeroMemory(state, sizeof(*state));
    ExInitializeFastMutex(&state->processLock);
    KeInitializeSpinLock(&state->sessionLock);
    ExInitializeRundownProtection(&state->sessionRundown);
    gC5State = state;
    status = FwpsRedirectHandleCreate0(&C5_WFP_PROVIDER_KEY, 0, &state->redirectHandle);
    if (!NT_SUCCESS(status)) {
        goto fail;
    }
    status = C5RegisterCallout(deviceObject, &C5_WFP_CALLOUT_V4_KEY, &state->calloutV4Id);
    if (!NT_SUCCESS(status)) {
        goto fail;
    }
    status = C5RegisterCallout(deviceObject, &C5_WFP_CALLOUT_V6_KEY, &state->calloutV6Id);
    if (!NT_SUCCESS(status)) {
        goto fail;
    }
    status = PsSetCreateProcessNotifyRoutine(C5ProcessNotify, FALSE);
    if (!NT_SUCCESS(status)) {
        goto fail;
    }
    state->processNotifyRegistered = TRUE;
    status = IoCreateSymbolicLink(&dosDeviceName, &deviceName);
    if (!NT_SUCCESS(status)) {
        goto fail;
    }
    driverObject->MajorFunction[IRP_MJ_CREATE] = C5Dispatch;
    driverObject->MajorFunction[IRP_MJ_CLOSE] = C5Dispatch;
    driverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = C5Dispatch;
    driverObject->DriverUnload = C5Unload;
    deviceObject->Flags &= ~DO_DEVICE_INITIALIZING;
    return STATUS_SUCCESS;

fail:
    C5Unload(driverObject);
    return status;
}
