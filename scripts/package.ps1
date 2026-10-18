param(
    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release'
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $projectRoot 'target'
$packageRoot = Join-Path $targetRoot 'package'

Push-Location -LiteralPath $projectRoot
try {
    $cargoArguments = @('build', '-p', 'moriq-desktop', '--locked', '--target-dir', $targetRoot)
    if ($Configuration -eq 'Release') {
        $cargoArguments += '--release'
    }
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw 'MORIQ build failed; the previous package was kept.'
    }

    $binary = Join-Path $targetRoot "$($Configuration.ToLowerInvariant())/moriq-desktop.exe"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Windows executable not found: $binary"
    }
    New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
    $executable = Join-Path $packageRoot 'MORIQ.exe'
    $archive = Join-Path $packageRoot 'MORIQ-Windows.zip'
    $stagedExecutable = Join-Path $packageRoot 'MORIQ.pending.exe'
    $stagedArchive = Join-Path $packageRoot 'MORIQ-Windows.pending.zip'
    try {
        Copy-Item -LiteralPath $binary -Destination $stagedExecutable -Force
        # Build the archive from the final filename in a separate staging directory.
        $stagingRoot = Join-Path $packageRoot ([Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $stagingRoot | Out-Null
        try {
            Copy-Item -LiteralPath $stagedExecutable -Destination (Join-Path $stagingRoot 'MORIQ.exe')
            Compress-Archive -LiteralPath (Join-Path $stagingRoot 'MORIQ.exe') -DestinationPath $stagedArchive -Force
        }
        finally {
            # Both targets are fixed descendants of the freshly created staging directory.
            Remove-Item -LiteralPath (Join-Path $stagingRoot 'MORIQ.exe') -Force -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $stagingRoot -Force -ErrorAction SilentlyContinue
        }
        Move-Item -LiteralPath $stagedExecutable -Destination $executable -Force
        Move-Item -LiteralPath $stagedArchive -Destination $archive -Force
    }
    finally {
        Remove-Item -LiteralPath $stagedExecutable -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $stagedArchive -Force -ErrorAction SilentlyContinue
    }
    Write-Output "MORIQ ($Configuration): $executable"
    Write-Output "Portable ZIP: $archive"
}
finally {
    Pop-Location
}
