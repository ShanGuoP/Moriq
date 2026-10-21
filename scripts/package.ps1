param(
    [ValidateSet('Release','Debug')][string]$Configuration='Release',
    [switch]$Offline,
    [switch]$SkipChecks,
    [switch]$PublicRelease
)
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$targetRoot=Join-Path $projectRoot 'target'
$packageRoot=Join-Path $targetRoot 'package'
$reportsRoot=Join-Path $targetRoot 'package-reports'
. (Join-Path $PSScriptRoot 'ai-runtime-files.ps1')
$previousBuildId=$env:MORIQ_BUILD_ID
if($PublicRelease){
    $sourceStatus=& git -C $projectRoot status --porcelain
    if($sourceStatus){throw 'Public release requires a clean working tree; development package remains available without -PublicRelease'}
}
& (Join-Path $PSScriptRoot 'prepare-codecs.ps1') -Offline:$Offline
& (Join-Path $PSScriptRoot 'prepare-ai-runtime.ps1') -Offline:$Offline
Push-Location -LiteralPath $projectRoot
try {
    & python (Join-Path $PSScriptRoot 'release-metadata.py') identity
    if($LASTEXITCODE -ne 0){throw 'Source identity generation failed'}
    $buildInfo=Get-Content -LiteralPath (Join-Path $targetRoot 'build-info.json') -Raw | ConvertFrom-Json
    if($PublicRelease){
        if($buildInfo.dirty){throw 'Public release requires a clean working tree'}
        & git describe --exact-match --tags HEAD
        if($LASTEXITCODE -ne 0){throw 'Public release requires an exact source tag'}
        if($SkipChecks){throw 'Public release cannot skip validation'}
    }
    $validation=@()
    if(-not $SkipChecks){
        & python (Join-Path $PSScriptRoot 'check-source-style.py')
        if($LASTEXITCODE -ne 0){throw 'Source style validation failed'}
        $validation+=@{Command='python scripts/check-source-style.py';Passed=$true}

        foreach($checkArgs in @(
            @('fmt','--all','--','--check'),
            @('clippy','--workspace','--all-targets','--all-features','--locked','--','-D','warnings'),
            @('test','--workspace','--all-targets','--all-features','--locked')
        )){
            & cargo @checkArgs
            if($LASTEXITCODE -ne 0){throw ('Validation failed: cargo '+($checkArgs -join ' '))}
            $validation+=@{Command=('cargo '+($checkArgs -join ' '));Passed=$true}
        }
    }
    $previousBuildId=$env:MORIQ_BUILD_ID
    $env:MORIQ_BUILD_ID=$buildInfo.build_id
    $arguments=@('build','-p','moriq-desktop','--locked','--target-dir',$targetRoot)
    if($Configuration -eq 'Release'){$arguments+='--release'}
    & cargo @arguments
    if($LASTEXITCODE -ne 0){throw 'Build failed; previous package retained'}
    $binary=Join-Path $targetRoot "$($Configuration.ToLowerInvariant())/moriq-desktop.exe"
    if(-not(Test-Path -LiteralPath $binary -PathType Leaf)){throw "Missing executable: $binary"}
    New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
    $stagingRoot=Join-Path $packageRoot ([Guid]::NewGuid().ToString('N'))
    $archive=Join-Path $packageRoot 'Moriq-Space-Windows.zip'
    $pending=Join-Path $packageRoot 'Moriq-Space-Windows.pending.zip'
    New-Item -ItemType Directory -Path $stagingRoot | Out-Null
    try {
        Copy-Item -LiteralPath $binary -Destination (Join-Path $stagingRoot 'Moriq Space.exe')
        Copy-Item -LiteralPath (Join-Path $targetRoot 'codecs/runtime') -Destination (Join-Path $stagingRoot 'codecs') -Recurse
        $codecChecksums=Get-Content -LiteralPath (Join-Path $stagingRoot 'codecs/checksums.json') -Raw | ConvertFrom-Json
        foreach($entry in $codecChecksums.PSObject.Properties){
            $runtimeFile=Join-Path $stagingRoot ('codecs/'+$entry.Name)
            if(-not(Test-Path -LiteralPath $runtimeFile -PathType Leaf)){throw "Codec package is missing manifest entry: $($entry.Name)"}
            if((Get-FileHash -LiteralPath $runtimeFile -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value){throw "Codec package checksum mismatch: $($entry.Name)"}
        }
        $ai=Join-Path $stagingRoot 'ai'
        New-Item -ItemType Directory -Path $ai | Out-Null
        foreach($name in $aiRuntimeEntries){
            Copy-Item -LiteralPath (Join-Path $targetRoot "ai-runtime/$name") -Destination $ai -Recurse
        }
        # Check the actual shipping folder, not just the development runtime.
        # A missing entry must fail here before producing an unusable ZIP.
        $runtimeChecksums=Get-Content -LiteralPath (Join-Path $ai 'checksums.json') -Raw | ConvertFrom-Json
        foreach($entry in $runtimeChecksums.PSObject.Properties){
            $runtimeFile=Join-Path $ai $entry.Name
            if(-not(Test-Path -LiteralPath $runtimeFile -PathType Leaf)){throw "AI package is missing manifest entry: $($entry.Name)"}
            if((Get-FileHash -LiteralPath $runtimeFile -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value){throw "AI package checksum mismatch: $($entry.Name)"}
        }
        # User packages contain the application, runtimes and retained notices only.
        # Build identity and audit inventories stay beside build outputs, outside the ZIP.
        $licensesRoot=Join-Path $stagingRoot 'licenses'
        New-Item -ItemType Directory -Path $licensesRoot -Force | Out-Null
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        foreach($name in @('LICENSE','NOTICE','THIRD_PARTY_NOTICES.md')){
            Copy-Item -LiteralPath (Join-Path $projectRoot $name) -Destination $licensesRoot
        }
        Copy-Item -LiteralPath (Join-Path $targetRoot 'build-info.json') -Destination (Join-Path $reportsRoot 'BUILD.json')
        ($buildInfo.version+' '+$buildInfo.build_id) | Set-Content -LiteralPath (Join-Path $reportsRoot 'VERSION') -Encoding utf8
        & python (Join-Path $PSScriptRoot 'release-metadata.py') notices $licensesRoot $reportsRoot
        if($LASTEXITCODE -ne 0){throw 'Third-party notice generation failed'}
        $auditTool=Join-Path $targetRoot 'audit-tools/bin/cargo-audit.exe'
        if(Test-Path -LiteralPath $auditTool){
            $auditArgs=@('audit','--json','--db',(Join-Path $targetRoot 'advisory-db'))
            if($Offline){$auditArgs+='--no-fetch'}
            $securityText=(& $auditTool @auditArgs) -join "`n"
            if($LASTEXITCODE -ne 0){throw 'Dependency security audit failed'}
            $security=$securityText | ConvertFrom-Json
            if($security.vulnerabilities.found -or $security.warnings.unsound.Count -gt 0){throw 'Dependency security audit reports vulnerabilities or unsoundness'}
            $securityText | Set-Content -LiteralPath (Join-Path $reportsRoot 'security-audit.json') -Encoding utf8
            $databaseCommit=$security.database.'last-commit'
            if(-not $databaseCommit){
                $databaseCommit=(& git -C (Join-Path $targetRoot 'advisory-db') rev-parse HEAD)
                if($LASTEXITCODE -ne 0){throw 'Cannot determine security advisory database revision'}
            }
            $validation+=@{Command='cargo-audit';Passed=$true;DatabaseCommit=$databaseCommit;UnmaintainedWarnings=$security.warnings.unmaintained.Count}
        } elseif($PublicRelease){throw 'Public release requires cargo-audit at target/audit-tools/bin/cargo-audit.exe'}
        if($PublicRelease){
            $gaps=Get-Content -LiteralPath (Join-Path $reportsRoot 'license-gaps.json') -Raw | ConvertFrom-Json
            if($gaps.Count -gt 0){throw 'Missing upstream license texts; inspect license-gaps.json'}
            if((Get-AuthenticodeSignature -LiteralPath $binary).Status -ne 'Valid'){throw 'Public release requires a valid Authenticode signature'}
            $sourceManifest=Join-Path $targetRoot 'corresponding-sources/verified.json'
            if(-not(Test-Path -LiteralPath $sourceManifest)){throw 'Public release requires verified corresponding-source archives'}
            & python (Join-Path $PSScriptRoot 'validate-source-archive.py') (Split-Path -Parent $sourceManifest)
            if($LASTEXITCODE -ne 0){throw 'Corresponding-source inventory validation failed'}
            Copy-Item -LiteralPath (Split-Path -Parent $sourceManifest) -Destination (Join-Path $licensesRoot 'corresponding-sources') -Recurse
        }
        $expectedRoot=@('Moriq Space.exe','ai','codecs','licenses')
        $actualRoot=@(Get-ChildItem -LiteralPath $stagingRoot -Force | ForEach-Object {$_.Name})
        if(Compare-Object $expectedRoot $actualRoot){throw 'Unexpected portable package root entries'}
        & python (Join-Path $PSScriptRoot 'release-metadata.py') identity
        $finalSource=Get-Content -LiteralPath (Join-Path $targetRoot 'build-info.json') -Raw | ConvertFrom-Json
        if($finalSource.source_sha256 -ne $buildInfo.source_sha256){throw 'Source changed during packaging; previous package retained'}
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        if(Test-Path -LiteralPath $pending){Remove-Item -LiteralPath $pending -Force}
        [IO.Compression.ZipFile]::CreateFromDirectory($stagingRoot,$pending,[IO.Compression.CompressionLevel]::Optimal,$false)
        Move-Item -LiteralPath $pending -Destination $archive -Force
        $hash=(Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        ($hash+'  Moriq-Space-Windows.zip') | Set-Content -LiteralPath (Join-Path $packageRoot 'SHA256SUMS.txt') -Encoding ascii
        @{Archive='Moriq-Space-Windows.zip';Sha256=$hash;Bytes=(Get-Item -LiteralPath $archive).Length;BuildId=$buildInfo.build_id;Commit=$buildInfo.commit;SourceSha256=$buildInfo.source_sha256;Dirty=$buildInfo.dirty;PublicRelease=[bool]$PublicRelease;Validation=$validation;BinarySha256=(Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $packageRoot 'verification.json') -Encoding utf8
        Write-Output "Portable package: $archive"
    } finally {
        $resolvedStage=[IO.Path]::GetFullPath($stagingRoot)
        $resolvedParent=[IO.Path]::GetFullPath($packageRoot)+[IO.Path]::DirectorySeparatorChar
        if(-not $resolvedStage.StartsWith($resolvedParent,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid package staging path'}
        if(Test-Path -LiteralPath $resolvedStage){Remove-Item -LiteralPath $resolvedStage -Recurse -Force}
        if(Test-Path -LiteralPath $pending){Remove-Item -LiteralPath $pending -Force}
    }
} finally {$env:MORIQ_BUILD_ID=$previousBuildId;Pop-Location}
