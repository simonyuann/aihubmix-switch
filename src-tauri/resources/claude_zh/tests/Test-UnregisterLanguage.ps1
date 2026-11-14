param([string]$InstallerPath = (Join-Path (Split-Path -Parent $PSScriptRoot) 'scripts\install_windows.ps1'))

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Load-InstallerFunctions.ps1') @(
    'Resolve-FrontendAssetsDir',
    'Get-FrontendJsFilesContaining',
    'Unregister-Language'
)

$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$BaseLanguageList = '["en-US","de-DE","fr-FR","ko-KR","ja-JP","es-419","es-ES","it-IT","hi-IN","pt-BR","id-ID"'
$LanguageListPattern = [regex]::Escape($BaseLanguageList) + '(?:(?:,"zh-CN")|(?:,"zh-TW")|(?:,"zh-HK"))*\]'

$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('claude-zh-unregister-test-' + [guid]::NewGuid())
$assets = Join-Path $fixtureRoot 'ion-dist\assets\v1'
$file = Join-Path $assets 'fixture.js'
$nativeFile = Join-Path $assets 'cff2bf6cb-native-only.js'
New-Item -ItemType Directory -Path $assets -Force | Out-Null

try {
    # Native objects matching the exact patterns from cff2bf6cb and shared-14
    $nativeContent = ';const localeMap={"uk-UA":Zn,"zh-CN":Qn,"zh-TW":$n};const nativeList=["de","es","fr","it","ja","ko","pt","ru","zh-CN","zh-TW","id"];'
    [System.IO.File]::WriteAllText($nativeFile, $nativeContent, $Utf8NoBom)

    foreach ($suffix in @(',"zh-CN"', ',"zh-TW","zh-HK"', ',"zh-CN","zh-TW","zh-CN"', '')) {
        $inputText = 'const whitelist=' + $BaseLanguageList + $suffix + '];const secondWhitelist=' + $BaseLanguageList + $suffix + ']' + $nativeContent
        $expected = 'const whitelist=' + $BaseLanguageList + '];const secondWhitelist=' + $BaseLanguageList + ']' + $nativeContent

        [System.IO.File]::WriteAllText($file, $inputText, $Utf8NoBom)
        Unregister-Language $fixtureRoot

        $actual = [System.IO.File]::ReadAllText($file)
        if ($actual -cne $expected) {
            throw "Unregister must only reset the supported-language whitelist, preserving native objects."
        }

        # Idempotence test
        Unregister-Language $fixtureRoot
        if ([System.IO.File]::ReadAllText($file) -cne $expected) {
            throw "Unregister must be idempotent."
        }

        # Native-only file must remain completely untouched
        if ([System.IO.File]::ReadAllText($nativeFile) -cne $nativeContent) {
            throw "Native-only files (cff2bf6cb/shared-14) without the whitelist anchor must stay 100% unchanged."
        }
    }
    Write-Host "PASS: Unregister-Language correctly preserves native locale mappings and only resets whitelists." -ForegroundColor Green
}
finally {
    Remove-Item -LiteralPath $file -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $nativeFile -Force -ErrorAction SilentlyContinue
    foreach ($dir in @($assets, (Split-Path $assets -Parent), (Join-Path $fixtureRoot 'ion-dist'), $fixtureRoot)) {
        Remove-Item -LiteralPath $dir -Force -ErrorAction SilentlyContinue
    }
}
