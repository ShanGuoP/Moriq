param(
    [ValidateSet('Release','Debug')][string]$Configuration='Release',
    [switch]$Offline
)
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$targetRoot=Join-Path $projectRoot 'target'
$packageRoot=Join-Path $targetRoot 'package'
& (Join-Path $PSScriptRoot 'prepare-codecs.ps1') -Offline:$Offline
& (Join-Path $PSScriptRoot 'prepare-ai-runtime.ps1') -Offline:$Offline
Push-Location -LiteralPath $projectRoot
try {
    $arguments=@('build','-p','moriq-desktop','--locked','--target-dir',$targetRoot)
    if($Configuration -eq 'Release'){$arguments+='--release'}
    & cargo @arguments
    if($LASTEXITCODE -ne 0){throw 'Build failed; previous package retained'}
    $binary=Join-Path $targetRoot "$($Configuration.ToLowerInvariant())/moriq-desktop.exe"
    if(-not(Test-Path -LiteralPath $binary -PathType Leaf)){throw "Missing executable: $binary"}
    New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
    $stagingRoot=Join-Path $packageRoot ([Guid]::NewGuid().ToString('N'))
    $archive=Join-Path $packageRoot 'MORIQ-Windows.zip'
    $pending=Join-Path $packageRoot 'MORIQ-Windows.pending.zip'
    New-Item -ItemType Directory -Path $stagingRoot | Out-Null
    try {
        Copy-Item -LiteralPath $binary -Destination (Join-Path $stagingRoot 'MORIQ.exe')
        Copy-Item -LiteralPath (Join-Path $targetRoot 'codecs/runtime') -Destination (Join-Path $stagingRoot 'codecs') -Recurse
        $ai=Join-Path $stagingRoot 'ai'
        New-Item -ItemType Directory -Path $ai | Out-Null
        foreach($name in @('node.exe','Node-LICENSE.txt','worker.mjs','faces.mjs','models.json','package.json','package-lock.json','provenance.json','node_modules')){
            Copy-Item -LiteralPath (Join-Path $targetRoot "ai-runtime/$name") -Destination $ai -Recurse
        }
        Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $stagingRoot
        Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD_PARTY_NOTICES.md') -Destination $stagingRoot
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        if(Test-Path -LiteralPath $pending){Remove-Item -LiteralPath $pending -Force}
        [IO.Compression.ZipFile]::CreateFromDirectory($stagingRoot,$pending,[IO.Compression.CompressionLevel]::Optimal,$false)
        Move-Item -LiteralPath $pending -Destination $archive -Force
        Write-Output "Portable package: $archive"
    } finally {
        $resolvedStage=[IO.Path]::GetFullPath($stagingRoot)
        $resolvedParent=[IO.Path]::GetFullPath($packageRoot)+[IO.Path]::DirectorySeparatorChar
        if(-not $resolvedStage.StartsWith($resolvedParent,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid package staging path'}
        if(Test-Path -LiteralPath $resolvedStage){Remove-Item -LiteralPath $resolvedStage -Recurse -Force}
        if(Test-Path -LiteralPath $pending){Remove-Item -LiteralPath $pending -Force}
    }
} finally {Pop-Location}
