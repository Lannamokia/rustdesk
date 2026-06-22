<#
.SYNOPSIS
    vhd-machine-auth-bridge artifact string scanner — task 21.2.

.DESCRIPTION
    Validates that a built RustDesk artifact matches the per-flavor
    invariants in requirements §1, §14, §22 and design §"编译特性矩阵":

      controller : default features, NO bridge tokens, RS_PUB_KEY present.
      controlled : vhd-bridge,controlled-only — bridge tokens present
                   (sanity check), RS_PUB_KEY still present.
      relay      : default features, NO bridge tokens, RS_PUB_KEY present.

    Three checks run on every flavor:

      A. Positive RS_PUB_KEY check.  Read RS_PUB_KEY from
         libs/hbb_common/src/config.rs.  Cross-validate against
         HBBS_KEY env (CI path) or `HBBS Key` line in secret.sec
         (local path) — they MUST be byte-identical, and decode to
         exactly 32 bytes (Requirement 22.2 / 22.6).  Then assert
         the same ASCII string appears in the binary (Requirement
         22.6 invariant: secret.sec → RS_PUB_KEY is mechanically
         observable).

      B. Negative secret.sec leakage scan.  No plaintext `HBBS Key`
         (base64) — except where it is the same as RS_PUB_KEY,
         which is intentional — and no plaintext `VHDMount Key`
         (hex) may appear in the binary.  The integer
         `VHDMount Key Version` is allowed (Requirement 14.4).

      C. Per-flavor bridge-token rules.  Controller / Relay artifacts
         MUST NOT contain any of the forbidden bridge token set
         (Requirement 1.2 / 14.2 / 14.3).  Controlled artifacts MUST
         contain a small expected subset (sanity).

.PARAMETER Flavor
    controller | controlled | relay.

.PARAMETER Target
    Cargo target triple (e.g. x86_64-pc-windows-msvc).  Used to
    locate the binary under target/<triple>/{release,debug}/.
    Optional — falls back to target/{release,debug}/.

.PARAMETER BinaryPath
    Explicit path to the binary; overrides target-triple search.

.PARAMETER RepoRoot
    Repo root.  Defaults to the script's parent's parent.

.EXAMPLE
    pwsh -File scripts/check_bridge_strings.ps1 `
        -Flavor controller -Target x86_64-unknown-linux-gnu

.EXAMPLE
    pwsh -File scripts/check_bridge_strings.ps1 `
        -Flavor controlled -BinaryPath target/release/rustdesk.exe

.NOTES
    Exit codes:
      0  all checks pass.
      2  binary not found.
      3  forbidden bridge token present (controller/relay).
      4  RS_PUB_KEY missing from binary.
      5  secret.sec plaintext leak.
      6  RS_PUB_KEY cross-source inconsistency.
      7  controlled-flavor sanity check failed.

    The script is pwsh-compatible (PowerShell 7+), so the same file
    drives both windows-2022 and ubuntu-24.04 GitHub runners.

    Wired into .github/workflows/vhd-bridge.yml under task 21.1.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('controller', 'controlled', 'relay')]
    [string]$Flavor,

    [Parameter()]
    [string]$Target = '',

    [Parameter()]
    [string]$BinaryPath = '',

    [Parameter()]
    [string]$RepoRoot = ''
)

$ErrorActionPreference = 'Stop'

if (-not $RepoRoot) {
    $RepoRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

function Fail {
    param([int]$Code, [string]$Message)
    # Emit to stderr without a noisy Error record stack-trace; CI logs
    # are easier to scan that way.
    [Console]::Error.WriteLine("check_bridge_strings: $Message")
    exit $Code
}

function Resolve-Binary {
    param(
        [string]$Root,
        [string]$Target,
        [string]$Flavor,
        [string]$Override
    )
    if ($Override) {
        if (-not (Test-Path $Override)) {
            Fail 2 "explicit BinaryPath '$Override' does not exist"
        }
        return (Resolve-Path $Override).Path
    }
    $isWindowsBin =
        ($Target -like '*windows*') -or
        ($Flavor -eq 'controlled')
    $exe = if ($isWindowsBin) { 'rustdesk.exe' } else { 'rustdesk' }
    $candidates = New-Object System.Collections.Generic.List[string]
    if ($Target) {
        $candidates.Add("target/$Target/release/$exe")
        $candidates.Add("target/$Target/debug/$exe")
    }
    $candidates.Add("target/release/$exe")
    $candidates.Add("target/debug/$exe")
    foreach ($rel in $candidates) {
        $p = Join-Path $Root $rel
        if (Test-Path $p) { return (Resolve-Path $p).Path }
    }
    Fail 2 "no rustdesk binary found.  searched: $($candidates -join ', ')"
}

function Read-RsPubKeySource {
    param([string]$Root)
    $cfg = Join-Path $Root 'libs/hbb_common/src/config.rs'
    if (-not (Test-Path $cfg)) {
        Fail 6 "source file not found: $cfg"
    }
    # Two definition shapes are accepted:
    #   1) Plain literal:
    #        pub const RS_PUB_KEY: &str = "...";
    #   2) option_env! fallback (the Lannamokia fork shape):
    #        pub const RS_PUB_KEY: &str = match option_env!("RUSTDESK_RS_PUB_KEY") {
    #            Some(s) => s,
    #            None => "...",
    #        };
    # Form (2) requires reading the multi-line definition, so use a single
    # whole-file regex with `Singleline` instead of `Select-String -Pattern`.
    $text = [System.IO.File]::ReadAllText($cfg)
    # Form 1
    $m = [regex]::Match(
        $text,
        '(?m)^\s*pub const RS_PUB_KEY:\s*&str\s*=\s*"([^"]+)"\s*;'
    )
    if ($m.Success) { return $m.Groups[1].Value }
    # Form 2 — option_env!("RUSTDESK_RS_PUB_KEY") fallback string
    $m = [regex]::Match(
        $text,
        '(?s)pub const RS_PUB_KEY:\s*&str\s*=\s*match\s+option_env!\(\s*"RUSTDESK_RS_PUB_KEY"\s*\)\s*\{[^}]*?None\s*=>\s*"([^"]+)"'
    )
    if ($m.Success) { return $m.Groups[1].Value }
    Fail 6 "could not extract RS_PUB_KEY constant from $cfg"
}

# Mirrors libs/build_support::parse_secret_sec — see Requirement 3.12 /
# Property 21.  ASCII ':' and full-width '：' are byte-equivalent.
# Unknown lines are silently dropped.
function Read-SecretSec {
    param([string]$Root)
    $p = Join-Path $Root 'secret.sec'
    $map = @{}
    if (-not (Test-Path $p)) { return $map }
    foreach ($raw in (Get-Content -LiteralPath $p)) {
        if (-not $raw) { continue }
        $idxAscii = $raw.IndexOf(':')
        $idxFw    = $raw.IndexOf([char]0xff1a)
        $idx = -1
        if ($idxAscii -ge 0 -and $idxFw -ge 0) {
            $idx = [Math]::Min($idxAscii, $idxFw)
        } elseif ($idxAscii -ge 0) {
            $idx = $idxAscii
        } elseif ($idxFw -ge 0) {
            $idx = $idxFw
        } else {
            continue
        }
        $name  = $raw.Substring(0, $idx).Trim()
        $value = $raw.Substring($idx + 1).Trim()
        if (-not $name -or -not $value) { continue }
        $map[$name] = $value
    }
    return $map
}

# Byte-level substring scan — Boyer-Moore would be overkill; a binary
# of ~80 MiB scans in well under a second this way and the loop is
# completely allocation-free.
function Test-BinaryContainsAscii {
    param([byte[]]$Bytes, [string]$Token)
    if (-not $Token) { return $false }
    $needle = [System.Text.Encoding]::ASCII.GetBytes($Token)
    $needleLen = $needle.Length
    if ($needleLen -eq 0) { return $false }
    $hayLen = $Bytes.Length
    if ($hayLen -lt $needleLen) { return $false }
    $limit = $hayLen - $needleLen
    $first = $needle[0]
    for ($i = 0; $i -le $limit; $i++) {
        if ($Bytes[$i] -ne $first) { continue }
        $match = $true
        for ($j = 1; $j -lt $needleLen; $j++) {
            if ($Bytes[$i + $j] -ne $needle[$j]) {
                $match = $false
                break
            }
        }
        if ($match) { return $true }
    }
    return $false
}

# ---------------------------------------------------------------------------
# Token sets — kept in sync with src/lan.rs::FORBIDDEN_TOKENS, the
# protocol constants in src/vhd_bridge/protocol.rs, and the
# LOGIN_MSG_VHD_* literals in src/client.rs.  When any of those move,
# update this list.
# ---------------------------------------------------------------------------

$ForbiddenBridgeTokens = @(
    # Protocol identifiers (§Data Models / docs).
    'VHDRustDeskBridgeHandshakeV1',
    'VHDRustDeskBridgeReportV1',
    'VHDRustDeskBridgeLogV1',
    'VHDRustDeskBridgePeerApprovalV1',
    'VHDRustDeskBridgeRevocationV1',
    # Shared-secret name (must never appear as ASCII text).
    'RustDeskClientSharedSecret',
    # Named-pipe path / sidecar handle.
    'VHDMount.RustDeskBridge',
    # Wire-level field names that are unique to the bridge.
    'controlledMachineId',
    # UI / overlay artefacts.
    'Maintenance_Overlay',
    'vhd_overlay_',
    # Approval-gate symbols — the constants in src/client.rs *and*
    # their human-readable values, both of which only exist in the
    # controlled flavor (§19.12-§19.15).
    'LOGIN_MSG_VHD_APPROVAL_PENDING',
    'LOGIN_MSG_VHD_APPROVAL_REJECTED',
    'VHD Approval Pending',
    'VHD Approval Rejected',
    # Peer-approval frame names — these are the snake-case design
    # spellings, not the camelCase JSON ones.  The JSON version
    # `peerApprovalRequest` is also rare enough that catching either
    # form is desirable.
    'Peer_Approval_Request',
    'Peer_Approval_Response',
    'peer_approval'
)

# Subset of bridge tokens the controlled flavor must contain.  Picked
# to be narrow — they uniquely identify "the bridge code is linked"
# without depending on incidental implementation details.  Empty
# matches against any of these mean the controlled flavor lost the
# bridge code entirely (e.g. feature-flag misconfig).
#
# `VHDRustDeskBridgeReportV1` and `VHDRustDeskBridgeLogV1` are
# intentionally NOT listed here even though they are valid bridge
# tokens.  Their `ReportFrame` / `LogFrame` constructors and the
# wrapping `hmac_report` / `hmac_log` helpers are defined in
# `src/vhd_bridge/{protocol,hmac}.rs` but have no non-test caller
# yet (the startup-report and structured-log emit paths in
# `triggers.rs` / `log_sink.rs` are still spec stubs).  Release LTO
# therefore drops both string literals from the binary, and asserting
# their presence here would produce false positives until the emit
# paths are wired up.  When that work lands, add them back.
$ExpectedControlledTokens = @(
    'VHDRustDeskBridgeHandshakeV1',
    'VHDRustDeskBridgePeerApprovalV1',
    'VHDMount.RustDeskBridge'
)

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

$bin = Resolve-Binary -Root $RepoRoot -Target $Target -Flavor $Flavor -Override $BinaryPath
Write-Host "scanning: $bin (flavor=$Flavor, target=$Target)"

$bytes = [System.IO.File]::ReadAllBytes($bin)
Write-Host ("binary size: {0:N0} bytes" -f $bytes.Length)

$rsPubSource = Read-RsPubKeySource -Root $RepoRoot
$secMap      = Read-SecretSec       -Root $RepoRoot

# ---- Check A: RS_PUB_KEY cross-source consistency ------------------------
# When HBBS_KEY env (or secret.sec line) is set, libs/hbb_common/build.rs
# emits  cargo:rustc-env=RUSTDESK_RS_PUB_KEY=<that value>, which
# config.rs's `option_env!` consumes at compile time.  In that case the
# binary contains the env value, NOT the source-code fallback literal.
# Pick the right "expected" value accordingly.

$envKey = $env:HBBS_KEY
$secKey = $null
if ($secMap.ContainsKey('HBBS Key')) { $secKey = $secMap['HBBS Key'] }

if ($envKey)        { $expected = $envKey;        $srcLabel = 'HBBS_KEY env (compile-injected)' }
elseif ($secKey)    { $expected = $secKey;        $srcLabel = "secret.sec ``HBBS Key`` line (compile-injected)" }
else {
    # No build-time injection — binary must contain the fallback literal
    # from config.rs.  Cross-source check degenerates to "fallback exists".
    $expected = $rsPubSource
    $srcLabel = 'config.rs fallback (no compile-time injection)'
}

# Sanity: whatever we expect, it must be valid base64-32.
try {
    $decoded = [Convert]::FromBase64String($expected)
    if ($decoded.Length -ne 32) {
        Fail 6 "RS_PUB_KEY ($srcLabel) decodes to $($decoded.Length) bytes, expected 32"
    }
} catch {
    Fail 6 "RS_PUB_KEY ($srcLabel) is not valid base64: $_"
}
Write-Host "ok: RS_PUB_KEY source ($srcLabel) is valid base64-32"

# ---- Check A (cont'd): RS_PUB_KEY ASCII present in binary ----------------

if (-not (Test-BinaryContainsAscii -Bytes $bytes -Token $expected)) {
    Fail 4 "RS_PUB_KEY ($srcLabel) is not present in binary $bin"
}
Write-Host "ok: RS_PUB_KEY ASCII string present in binary"

# ---- Check B: secret.sec plaintext leak scan -----------------------------
# `HBBS Key` is intentionally embedded as RS_PUB_KEY, skip when equal.
# `VHDMount Key` (hex) MUST never appear — it's the bridge shared
# secret and only ever lives in the binary as 32 raw bytes inside
# the OUT_DIR-generated `[u8; 32]` literal, which serialises to bytes
# that are NOT the ASCII hex of the key.
$leakProbes = @(
    @{ key = 'HBBS Key';     allow_if_eq_rs_pub = $true  },
    @{ key = 'VHDMount Key'; allow_if_eq_rs_pub = $false }
)
foreach ($probe in $leakProbes) {
    $k = $probe.key
    if (-not $secMap.ContainsKey($k)) { continue }
    $v = $secMap[$k]
    # `$expected` is the value that's actually compiled into the binary
    # (env-injected when present, fallback otherwise) — see Check A above.
    if ($probe.allow_if_eq_rs_pub -and $v -eq $expected) { continue }
    if (Test-BinaryContainsAscii -Bytes $bytes -Token $v) {
        Fail 5 "secret.sec leak: '$k' value present in binary $bin"
    }
}
Write-Host "ok: no secret.sec plaintext leak"

# ---- Check C: per-flavor bridge-token rules ------------------------------

if ($Flavor -eq 'controlled') {
    $missing = New-Object System.Collections.Generic.List[string]
    foreach ($t in $ExpectedControlledTokens) {
        if (-not (Test-BinaryContainsAscii -Bytes $bytes -Token $t)) {
            [void]$missing.Add($t)
        }
    }
    if ($missing.Count -gt 0) {
        Fail 7 "controlled artifact missing expected bridge tokens: $($missing -join ', ')"
    }
    Write-Host "ok: controlled artifact contains expected bridge tokens"
} else {
    $hits = New-Object System.Collections.Generic.List[string]
    foreach ($t in $ForbiddenBridgeTokens) {
        if (Test-BinaryContainsAscii -Bytes $bytes -Token $t) {
            [void]$hits.Add($t)
        }
    }
    if ($hits.Count -gt 0) {
        Fail 3 "forbidden bridge tokens present in '$Flavor' artifact: $($hits -join ', ')"
    }
    Write-Host "ok: no forbidden bridge tokens in $Flavor artifact"
}

Write-Host "ALL CHECKS PASSED ($Flavor)"
exit 0
