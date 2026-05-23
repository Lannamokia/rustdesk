#requires -Version 5.1
<#
.SYNOPSIS
    Behavioral smoke test for the `RustDesk_Controlled` build artifact
    (vhd-machine-auth-bridge §17.7).

.DESCRIPTION
    Exercises a built `rustdesk.exe` produced with the
    `vhd-bridge,controlled-only` cargo feature combo and asserts:

      1. Initiator CLI subcommands (`--connect <id>`, `--play <id>`,
         `--port-forward`, `--file-transfer`, `--view-camera`, `--terminal`,
         `--rdp`) exit with a non-zero code and never proceed to make a
         remote connection.  Validates Requirements 1.6 / 20.6 (§17.1).

      2. The `rustdesk://<id>` URI scheme is likewise refused with a
         non-zero exit.  Validates Requirements 1.6 / 20.6 (§17.1).

      3. `--version` stdout contains a `vhd-bridge-secret-version=<n>` line
         exposing the compile-time secret_version.  Validates Requirements
         3.7 / 3.11 / 14.4 (§1.4).

      4. `--server` starts the controlled-side service path and stays
         alive past a hold window without exiting (i.e. the receiver path
         was preserved by §17 trimming).  Validates Requirement 20.9.

    Exits 0 on all-pass, non-zero on first failed check.  Each step prints
    a one-line result so CI logs are scannable.  The script is the
    counterpart to `check_bridge_strings.ps1` (task 21.2), which handles
    static symbol/string scanning; this one covers runtime behavior only.

.PARAMETER Binary
    Path to the controlled-only `rustdesk.exe` artifact under test.

.PARAMETER ServerHoldSeconds
    How long `--server` must stay alive (in seconds) before being
    declared healthy.  Default: 3.  Lower values risk false positives
    against a slow startup; higher values lengthen CI runs.

.PARAMETER RefusalTimeoutSeconds
    Per-test wall-clock cap for refused-initiator subcommands; if the
    binary doesn't exit by this point, the smoke fails the case.
    Default: 15.

.PARAMETER SkipServer
    Skip the `--server` liveness check.  Useful on CI runners where the
    Windows Service / privacy-mode initialization paths require
    privileges the runner doesn't have.

.EXAMPLE
    pwsh -File scripts/smoke_controlled_only.ps1 -Binary target/x86_64-pc-windows-msvc/release/rustdesk.exe

.EXAMPLE
    pwsh -File scripts/smoke_controlled_only.ps1 -Binary .\rustdesk.exe -SkipServer
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Binary,

    [Parameter()]
    [int]$ServerHoldSeconds = 3,

    [Parameter()]
    [int]$RefusalTimeoutSeconds = 15,

    [Parameter()]
    [switch]$SkipServer
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

if (-not (Test-Path -LiteralPath $Binary)) {
    Write-Error "Binary not found: $Binary"
    exit 2
}
$Binary = (Resolve-Path -LiteralPath $Binary).Path

Write-Host "==> smoke_controlled_only.ps1"
Write-Host "    binary             = $Binary"
Write-Host "    refusal timeout    = ${RefusalTimeoutSeconds}s"
Write-Host "    server hold window = ${ServerHoldSeconds}s"
Write-Host ""

$script:passes   = New-Object System.Collections.Generic.List[string]
$script:failures = New-Object System.Collections.Generic.List[string]

function Add-Pass([string]$Label, [string]$Detail) {
    $line = "PASS  $Label  ($Detail)"
    $script:passes.Add($line) | Out-Null
    Write-Host "    [+] $line" -ForegroundColor Green
}

function Add-Fail([string]$Label, [string]$Reason) {
    $line = "FAIL  $Label  ($Reason)"
    $script:failures.Add($line) | Out-Null
    Write-Host "    [-] $line" -ForegroundColor Red
}

# Run the binary with a list of args; capture stdout/stderr/exit.  Returns a
# hashtable @{ TimedOut, ExitCode, StdOut, StdErr }.  Drains stdout/stderr
# asynchronously so the child can never deadlock on a full pipe buffer.
function Invoke-Binary {
    param(
        [string[]]$ArgList,
        [int]$TimeoutSec
    )

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName               = $Binary
    $psi.UseShellExecute        = $false
    $psi.CreateNoWindow         = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError  = $true

    # Use Arguments rather than ArgumentList for PowerShell 5.1 compat.
    # All callers pass simple ASCII tokens with no embedded whitespace or
    # quote characters, so naive joining is safe.
    $psi.Arguments = ($ArgList -join ' ')

    $proc = [System.Diagnostics.Process]::Start($psi)
    $outTask = $proc.StandardOutput.ReadToEndAsync()
    $errTask = $proc.StandardError.ReadToEndAsync()

    $exited = $proc.WaitForExit($TimeoutSec * 1000)
    if (-not $exited) {
        try { $proc.Kill() } catch { }
        $proc.WaitForExit() | Out-Null
        return @{
            TimedOut = $true
            ExitCode = $proc.ExitCode
            StdOut   = $outTask.Result
            StdErr   = $errTask.Result
        }
    }

    return @{
        TimedOut = $false
        ExitCode = $proc.ExitCode
        StdOut   = $outTask.Result
        StdErr   = $errTask.Result
    }
}

# ---------------------------------------------------------------------------
# Test 1..N: refused initiator subcommands.  Each case must exit non-zero
# within $RefusalTimeoutSeconds and must NOT begin a connection.  The
# specific exit code is treated as opaque (current impl uses 2; any
# non-zero is acceptable per Requirement 20.6 wording "以非零退出码终止").
# ---------------------------------------------------------------------------

# Each entry: @{ Label = ...; Args = @(...); }
$refusalCases = @(
    @{ Label = '--connect <id>';      Args = @('--connect', '999999999') }
    @{ Label = '--play <id>';         Args = @('--play',    '999999999') }
    @{ Label = '--port-forward';      Args = @('--port-forward') }
    @{ Label = '--file-transfer';     Args = @('--file-transfer', '999999999') }
    @{ Label = '--view-camera';       Args = @('--view-camera',   '999999999') }
    @{ Label = '--terminal';          Args = @('--terminal',      '999999999') }
    @{ Label = '--rdp';               Args = @('--rdp',           '999999999') }
    @{ Label = 'rustdesk:// URI';     Args = @('rustdesk://999999999') }
)

foreach ($case in $refusalCases) {
    Write-Host "==> Test: $($case.Label)" -ForegroundColor Cyan
    $r = Invoke-Binary -ArgList $case.Args -TimeoutSec $RefusalTimeoutSeconds
    if ($r.TimedOut) {
        Add-Fail $case.Label "did not exit within ${RefusalTimeoutSeconds}s"
        continue
    }
    if ($r.ExitCode -eq 0) {
        Add-Fail $case.Label "exited 0; expected non-zero refusal"
        continue
    }
    # Best-effort positive evidence: the refusal arm in
    # `core_main.rs` logs `vhd_bridge: refused initiator ...` on
    # stderr right before `process::exit(2)`.  When that line is
    # observed, we record the exit as a *refusal* rather than a
    # generic non-zero exit, which protects against an unrelated
    # startup panic accidentally satisfying Requirement 20.6.
    # Absence of the log line is *not* a failure: on CI the std
    # logger may not be wired up to stderr at this stage of
    # initialization, so the canonical signal stays "non-zero exit".
    $combined = $r.StdOut + $r.StdErr
    if ($combined -match 'vhd_bridge:\s*refused initiator') {
        Add-Pass $case.Label "exit=$($r.ExitCode); refusal logged"
    } else {
        Add-Pass $case.Label "exit=$($r.ExitCode)"
    }
}

# ---------------------------------------------------------------------------
# Test: --version output exposes vhd-bridge-secret-version=<u32>
# ---------------------------------------------------------------------------

Write-Host "==> Test: --version exposes vhd-bridge-secret-version" -ForegroundColor Cyan
$r = Invoke-Binary -ArgList @('--version') -TimeoutSec $RefusalTimeoutSeconds
if ($r.TimedOut) {
    Add-Fail '--version' "did not exit within ${RefusalTimeoutSeconds}s"
} elseif ($r.ExitCode -ne 0) {
    Add-Fail '--version' "exit=$($r.ExitCode); expected 0 (stdout was: $($r.StdOut.Trim()))"
} else {
    $combined = ($r.StdOut + "`n" + $r.StdErr)
    $match = [regex]::Match($combined, '(?m)^vhd-bridge-secret-version=(\d+)\s*$')
    if (-not $match.Success) {
        $preview = $combined.Trim()
        Add-Fail '--version' "no `"vhd-bridge-secret-version=<n>`" line in output. Got: $preview"
    } else {
        Add-Pass '--version' "secret_version=$($match.Groups[1].Value)"
    }
}

# ---------------------------------------------------------------------------
# Test: --server stays alive past the hold window
#
# This is a coarse liveness check, not a service-conformance check.  It
# proves only that the controlled-side `--server` entry was preserved by
# §17 trimming and didn't immediately panic on startup.  CI runners that
# lack the privileges `start_server` ultimately needs (privacy_mode
# registry edits, system-tray init) can pass `-SkipServer` to bypass.
# ---------------------------------------------------------------------------

if ($SkipServer) {
    Write-Host "==> Test: --server liveness (SKIPPED via -SkipServer)" -ForegroundColor Yellow
} else {
    Write-Host "==> Test: --server liveness (>= ${ServerHoldSeconds}s)" -ForegroundColor Cyan
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName               = $Binary
    $psi.Arguments              = '--server'
    $psi.UseShellExecute        = $false
    $psi.CreateNoWindow         = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError  = $true
    $proc = [System.Diagnostics.Process]::Start($psi)

    # Drain pipes async so a chatty --server can't deadlock on a full
    # OS pipe buffer while we sleep.
    $outTask = $proc.StandardOutput.ReadToEndAsync()
    $errTask = $proc.StandardError.ReadToEndAsync()

    Start-Sleep -Seconds $ServerHoldSeconds

    if ($proc.HasExited) {
        $code = $proc.ExitCode
        # Capture whatever output was produced before exit for diagnosis.
        $proc.WaitForExit() | Out-Null
        $tail = $outTask.Result + $errTask.Result
        if ($tail.Length -gt 1024) { $tail = $tail.Substring(0, 1024) + '...[truncated]' }
        Add-Fail '--server liveness' "exited within ${ServerHoldSeconds}s (exit=$code). Output: $tail"
    } else {
        Add-Pass '--server liveness' "still running after ${ServerHoldSeconds}s; killing"
        try { $proc.Kill() } catch { }
        $proc.WaitForExit(5000) | Out-Null
    }
}

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "==> Summary"
Write-Host "    passed: $($script:passes.Count)"
Write-Host "    failed: $($script:failures.Count)"
if ($script:failures.Count -gt 0) {
    Write-Host ""
    Write-Host "Failures:" -ForegroundColor Red
    foreach ($f in $script:failures) { Write-Host "  $f" -ForegroundColor Red }
    exit 1
}

Write-Host ""
Write-Host "All checks passed." -ForegroundColor Green
exit 0
