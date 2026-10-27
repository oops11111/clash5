# WFP connect-redirection component

This is an unintegrated kernel component for M5, not an installable driver. It
contains a `DriverEntry`, an administrator-only control device, and runtime
registration for IPv4 and IPv6 ALE connect-redirection callouts. The application
does not package, load, or invoke it.

`C5RedirectTcp` redirects an already-selected TCP request to a session's loopback
relay, retaining the original destination in WFP-owned context. It leaves its own
previous redirections unchanged, rejects another redirector's connection, and
blocks a selected request if preparation fails. Every acquired writable request
is finalized, including rejection paths. The caller must verify the process
instance and TCP protocol, initialize the action to CONTINUE, and retain the
session and redirect handle with rundown protection for the entire call.

The service attaches one in-memory session through an administrator-only IOCTL,
including a session ID, loopback relay PID and port, and up to 128 target
PID/creation-time pairs. The driver verifies each live process instance before
publishing the session. A process notification removes a selected PID when that
instance exits, before the PID can be reused. The callout snapshots that state
under a spin lock and redirects only selected TCP traffic; other protocols
continue unchanged until the UDP path is implemented. Detach requires the same
nonzero session ID, so a stale controller cannot clear a newer session. The
driver waits for in-flight selected classifications before detach returns. Detach
is idempotent when no session exists, so startup recovery can clear an intent
written before driver attach. The
driver does not create WFP provider, callout, or filter policy objects: the
privileged service must do so transactionally and use the documented callout
keys in `driver.h`. Closing the control device is not
crash recovery; a service watchdog still has to remove its WFP filters and TUN
exclusions after a failure.

Protocol v3 replaces the exact target set under the same session ID without
removing WFP filters. The driver validates every new PID and creation time,
rejects a stale or foreign session, and publishes the set after checking that
each process is still live. The privileged service must persist the new set or
detach the session if persistence fails. This control path does not discover
children or make process-tree selection available.

Protocol v4 adds an administrator-only readback of the active session and its
exact target slots. The service compares this snapshot with its recovery record
before reporting the session active and during its monitor loop. An unavailable
or mismatched driver session requires recovery, even if WFP filters remain.

Explicit non-loopback local binds are currently rejected because a loopback-only
relay cannot satisfy their zone/address constraint. Supporting those binds, UDP,
target discovery, filter lifecycle, relay record handling, TUN exclusion, and
crash recovery remains required before M5 acceptance. This implementation uses
`ExAllocatePool2` and therefore targets Windows 10 version 2004 or later.
The linker marks the binary as requiring the Windows 10 native subsystem.

## Compile

Restore `Microsoft.Windows.WDK.x64` version `10.0.26100.6584` and its SDK
dependencies with NuGet. The command below expects SDK CPP version
`10.0.26100.1`, selected by the WDK dependency's lower bound. Use the x64 MSVC
toolchain from Visual Studio 2022:

```powershell
./build.ps1 -PackagesRoot <NuGet-packages-directory> `
    -MsvcRoot <VC/Tools/MSVC/version-directory> `
    -OutputDirectory <build-output-directory>
```

The script compiles with kernel mode, warnings-as-errors, and MSVC analysis. It
uses `/analyze:external-` and suppresses C28301, C28160, and C6387 only because
WDK 26100 emits them from its own `wdm.h` declarations. The allocation line also
suppresses C6014 because `FwpsApplyModifiedLayerData0` takes ownership of the
redirect context without an analyzer-visible contract. Other component diagnostics
still fail the build. It produces one object for each C source file and an
unsigned `clash5wfp.sys`; this build does not establish WFP runtime behavior.
Release driver signing, packaging, and installation are still pending. An
isolated Windows 10 host in test-signing mode has run temporary signed-driver
IPv4 and IPv6 TCP redirection probes, including one exact IPv4 target through
the relay and a generated two-hop Mihomo Chain. See
`docs/clash5/m5-wfp-redirection.md` for the evidence and remaining gates.

API constraints: [connect redirection](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/using-bind-or-connect-redirection),
[writable request finalization](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/fwpsk/nf-fwpsk-fwpsacquirewritablelayerdatapointer0).
