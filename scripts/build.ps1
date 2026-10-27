[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $outputDirectory = Join-Path $projectRoot 'dist'
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
    $executable = Join-Path $outputDirectory 'AI Skillview.exe'
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\ai-skillview.exe') -Destination $executable -Force
    Get-Item -LiteralPath $executable | Select-Object FullName, Length, LastWriteTime
}
finally {
    Pop-Location
}
