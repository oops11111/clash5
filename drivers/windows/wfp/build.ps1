param(
    [Parameter(Mandatory)][string]$PackagesRoot,
    [Parameter(Mandatory)][string]$MsvcRoot,
    [Parameter(Mandatory)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$wdk = Join-Path $PackagesRoot 'Microsoft.Windows.WDK.x64.10.0.26100.6584/c/Include/10.0.26100.0'
$wdkLibraries = Join-Path $PackagesRoot 'Microsoft.Windows.WDK.x64.10.0.26100.6584/c/Lib/10.0.26100.0/km/x64'
$sdk = Join-Path $PackagesRoot 'Microsoft.Windows.SDK.CPP.10.0.26100.1/c/Include/10.0.26100.0'
$compiler = Join-Path $MsvcRoot 'bin/Hostx64/x64/cl.exe'
$linker = Join-Path $MsvcRoot 'bin/Hostx64/x64/link.exe'
foreach ($required in @($compiler, $linker, "$wdk/km/fwpsk.h", "$sdk/shared/sdkddkver.h",
                           "$wdkLibraries/ntoskrnl.lib", "$wdkLibraries/fwpkclnt.lib",
                           "$wdkLibraries/wdmsec.lib", "$wdkLibraries/libcntpr.lib",
                           "$wdkLibraries/BufferOverflowK.lib")) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Missing build prerequisite: $required"
    }
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
foreach ($source in Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.c') {
    $output = Join-Path (Resolve-Path -LiteralPath $OutputDirectory).Path "$($source.BaseName).obj"
    $arguments = @(
        # WDK 26100 declares POOL_FLAGS before its SAL annotation in wdm.h. Keep
        # analysis enabled, but ignore that known header-only warning.
        '/nologo', '/c', '/kernel', '/Zl', '/W4', '/WX', '/analyze', '/analyze:external-',
        '/wd28301', '/wd28160', '/wd6387',
        '/D_AMD64_', '/DAMD64', '/D_WIN64', '/DNDIS630',
        '/D_WIN32_WINNT=0x0A00', '/DNTDDI_VERSION=0x0A000008',
        '/external:W0', "/external:I$wdk/km", "/external:I$wdk/km/crt",
        "/external:I$sdk/shared", "/external:I$MsvcRoot/include",
        "/external:I$sdk/ucrt", "/Fo$output", $source.FullName
    )
    & $compiler @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "WFP object compilation failed for $($source.Name) with exit code $LASTEXITCODE"
    }
}
$driverOutput = Join-Path (Resolve-Path -LiteralPath $OutputDirectory).Path 'clash5wfp.sys'
$objects = Get-ChildItem -LiteralPath $OutputDirectory -Filter '*.obj' | ForEach-Object FullName
$linkArguments = @(
    '/nologo', '/driver', '/subsystem:native,10.0', '/entry:DriverEntry', '/incremental:no',
    "/libpath:$wdkLibraries", "/out:$driverOutput"
) + $objects + @('ntoskrnl.lib', 'fwpkclnt.lib', 'wdmsec.lib', 'libcntpr.lib',
                 'BufferOverflowK.lib')
& $linker @linkArguments
if ($LASTEXITCODE -ne 0) {
    throw "WFP driver link failed with exit code $LASTEXITCODE"
}
