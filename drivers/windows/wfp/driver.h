#pragma once

#include "redirect.h"
#include <wdmsec.h>

#define C5_SESSION_PROTOCOL_VERSION 3U
#define C5_MAX_TARGET_PIDS 128U

#define IOCTL_C5_ATTACH_SESSION \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x801, METHOD_BUFFERED, FILE_WRITE_DATA)
#define IOCTL_C5_DETACH_SESSION \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x802, METHOD_BUFFERED, FILE_WRITE_DATA)
#define IOCTL_C5_UPDATE_TARGETS \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x803, METHOD_BUFFERED, FILE_WRITE_DATA)

typedef struct C5_TARGET_PROCESS {
    UINT64 pid;
    UINT64 createdAt;
} C5_TARGET_PROCESS;

typedef struct C5_ATTACH_REQUEST {
    UINT32 version;
    UINT32 size;
    UINT64 sessionId;
    UINT32 relayPid;
    UINT16 relayPort;
    UINT16 targetCount;
    C5_TARGET_PROCESS targets[C5_MAX_TARGET_PIDS];
} C5_ATTACH_REQUEST;

C_ASSERT(sizeof(C5_TARGET_PROCESS) == 16);
C_ASSERT(sizeof(C5_ATTACH_REQUEST) == 2072);

typedef struct C5_DETACH_REQUEST {
    UINT32 version;
    UINT32 size;
    UINT64 sessionId;
} C5_DETACH_REQUEST;

typedef struct C5_UPDATE_TARGETS_REQUEST {
    UINT32 version;
    UINT32 size;
    UINT64 sessionId;
    UINT32 targetCount;
    UINT32 reserved;
    C5_TARGET_PROCESS targets[C5_MAX_TARGET_PIDS];
} C5_UPDATE_TARGETS_REQUEST;

C_ASSERT(sizeof(C5_UPDATE_TARGETS_REQUEST) == 2072);

typedef struct C5_DRIVER_STATE {
    FAST_MUTEX processLock;
    KSPIN_LOCK sessionLock;
    EX_RUNDOWN_REF sessionRundown;
    BOOLEAN sessionActive;
    BOOLEAN processNotifyRegistered;
    C5_REDIRECT_SESSION session;
    UINT16 targetCount;
    C5_TARGET_PROCESS targets[C5_MAX_TARGET_PIDS];
    HANDLE redirectHandle;
    UINT32 calloutV4Id;
    UINT32 calloutV6Id;
} C5_DRIVER_STATE;

extern const GUID C5_WFP_PROVIDER_KEY;
extern const GUID C5_WFP_CALLOUT_V4_KEY;
extern const GUID C5_WFP_CALLOUT_V6_KEY;
extern const GUID C5_DEVICE_CLASS_KEY;
