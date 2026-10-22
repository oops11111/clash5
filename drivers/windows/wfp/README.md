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
including a session ID, loopback relay PID and port, and up to 128 target PIDs.
The callout snapshots that state under a spin lock and ignores any process that
is not in the selected set. It redirects only TCP; other protocols continue
unchanged until the UDP path is implemented. Detach requires the same nonzero
session ID, so a stale controller cannot clear a newer session. The driver does
not create WFP provider, callout, or filter policy objects: the privileged
service must do so transactionally and use the documented callout keys in
`driver.h`. Closing the control device is not
crash recovery; a service watchdog still has to remove its WFP filters and TUN
exclusions after a failure.

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
Driver signing, packaging, installation, and real Windows routing tests are
still pending.

API constraints: [connect redirection](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/using-bind-or-connect-redirection),
[writable request finalization](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/fwpsk/nf-fwpsk-fwpsacquirewritablelayerdatapointer0).
