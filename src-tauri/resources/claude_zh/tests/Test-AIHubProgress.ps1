$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/Load-InstallerFunctions.ps1" -FunctionNames @('Write-AIHubProgress', 'Test-AIHubCancellation')
$temporary = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    $AIHubProgressPath = Join-Path $temporary 'phase'
    Write-AIHubProgress 'extracting'
    $state = Get-Content -LiteralPath $AIHubProgressPath -Raw | ConvertFrom-Json
    if ($state.phase -ne 'extracting' -or $state.interruptible) { throw 'Invalid stage' }
    Write-AIHubProgress 'checking_backup' $true
    $state = Get-Content -LiteralPath $AIHubProgressPath -Raw | ConvertFrom-Json
    if ($state.phase -ne 'checking_backup' -or -not $state.interruptible) { throw 'Backup inspection should be cancellable' }
    Write-AIHubProgress 'restoring' $false 2 5
    $state = Get-Content -LiteralPath $AIHubProgressPath -Raw | ConvertFrom-Json
    if ($state.completed -ne 2 -or $state.total -ne 5 -or $state.interruptible) { throw 'Restore counters are invalid' }
    if (-not ('ClaudeZhPatch.FrontendPatcher' -as [type])) {
        Add-Type -TypeDefinition $script:FrontendPatcherCSharp
    }
    $paths = [string[]]@(0..16 | ForEach-Object {
        $path = Join-Path $temporary "$_.js"
        [IO.File]::WriteAllText($path, 'const message = "Hello";')
        $path
    })
    [ClaudeZhPatch.FrontendPatcher]::ProgressPath = $AIHubProgressPath
    $outputContents = [string[]]::new($paths.Count)
    [void][ClaudeZhPatch.FrontendPatcher]::PatchFiles($paths, [object[][]]@(), 4, $outputContents)
    $state = Get-Content -LiteralPath $AIHubProgressPath -Raw | ConvertFrom-Json
    if ($state.phase -ne 'translating' -or $state.completed -ne 17 -or $state.total -ne 17) { throw 'Invalid counters' }
    New-Item -ItemType File -Path (Join-Path $temporary 'cancel') | Out-Null
    $child = @"
function Write-AIHubProgress { ${function:Write-AIHubProgress} }
function Test-AIHubCancellation { ${function:Test-AIHubCancellation} }
`$AIHubProgressPath = '$($AIHubProgressPath.Replace("'", "''"))'
Test-AIHubCancellation
exit 99
"@
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($child))
    & "$PSHOME/powershell.exe" -NoProfile -NonInteractive -EncodedCommand $encoded
    if ($LASTEXITCODE -ne 130) { throw 'Cancellation did not stop the runner' }
    $state = Get-Content -LiteralPath $AIHubProgressPath -Raw | ConvertFrom-Json
    if ($state.phase -ne 'cancelled') { throw 'Cancellation was not reported' }
    Write-Output 'PASS: progress counters and cancellation'
}
finally {
    if ('ClaudeZhPatch.FrontendPatcher' -as [type]) { [ClaudeZhPatch.FrontendPatcher]::ProgressPath = $null }
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
