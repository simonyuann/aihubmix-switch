param([string]$Target = 'x86_64-pc-windows-msvc')
$ErrorActionPreference = 'Stop'

# tauri-build embeds Common Controls v6 into the app, but not lib test executables.
# The updater MockRuntime tests link Tauri's UI imports too. Activate v6 before loading them.
$buildOutput = & cargo test --release --manifest-path src-tauri/Cargo.toml --locked --target $Target --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Windows test compilation failed' }
$executables = @()
foreach ($line in $buildOutput) {
  try { $artifact = $line | ConvertFrom-Json } catch { continue }
  if ($artifact.reason -eq 'compiler-artifact' -and $artifact.profile.test -and $artifact.executable) {
    $executables += $artifact.executable
  }
}
if ($executables.Count -eq 0) { throw 'Cargo produced no Windows test executables' }
$sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$mt = Get-ChildItem "$sdkRoot/*/x64/mt.exe" | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $mt) { throw 'Windows SDK manifest tool is unavailable' }
$manifest = (Resolve-Path 'src-tauri/windows-test.manifest').Path
foreach ($executable in ($executables | Select-Object -Unique)) {
  & $mt.FullName -nologo -manifest $manifest "-outputresource:$executable;#1"
  if ($LASTEXITCODE -ne 0) { throw 'Could not activate Common Controls v6 for tests' }
  & $executable
  if ($LASTEXITCODE -ne 0) { throw "Windows native tests failed: $executable" }
}
