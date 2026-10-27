[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot

function Find-InnoCompiler {
    if ($env:INNO_SETUP_COMPILER) {
        if (-not (Test-Path -LiteralPath $env:INNO_SETUP_COMPILER -PathType Leaf)) {
            throw 'INNO_SETUP_COMPILER must point to an existing ISCC.exe file.'
        }
        return (Resolve-Path -LiteralPath $env:INNO_SETUP_COMPILER).Path
    }

    $candidates = @(
        (Join-Path $projectRoot '.tools\inno\ISCC.exe')
        (Join-Path $projectRoot '.tools\inno-nuget\tools\ISCC.exe')
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe')
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 7\ISCC.exe')
        (Join-Path $env:ProgramFiles 'Inno Setup 7\ISCC.exe')
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe')
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 7\ISCC.exe')
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }

    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    throw 'Inno Setup 6.3 or newer is required to build the installer. Install it from https://jrsoftware.org/isdl.php, or set INNO_SETUP_COMPILER to the full ISCC.exe path.'
}

$compiler = Find-InnoCompiler
Push-Location -LiteralPath $projectRoot
try {
    & (Join-Path $PSScriptRoot 'build.ps1')
    $metadataText = cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the Cargo package version.' }
    $metadata = $metadataText | ConvertFrom-Json
    $package = @($metadata.packages | Where-Object { $_.name -eq 'ai-skillview' })
    if ($package.Count -ne 1) { throw 'Expected exactly one ai-skillview package.' }

    $version = $package[0].version
    & $compiler "/DAppVersion=$version" "/DProjectRoot=$projectRoot" (Join-Path $projectRoot 'installer\ai-skillview.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
    Get-Item -LiteralPath (Join-Path $projectRoot 'dist\AI Skillview Setup.exe') |
        Select-Object FullName, Length, LastWriteTime
}
finally {
    Pop-Location
}
