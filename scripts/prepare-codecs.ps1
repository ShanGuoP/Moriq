param([switch]$Offline)
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$codecRoot=Join-Path $projectRoot 'target/codecs'
New-Item -ItemType Directory -Force -Path $codecRoot | Out-Null
$artifacts=@(
    @{Name='libraw'; Url='https://www.libraw.org/data/LibRaw-0.22.2-Win64.zip'; Sha256='AC64FA12BB00A7581332D4C6AB918C0533FB3F119D6B668D47A6875410DCA948'},
    @{Name='ffmpeg'; Url='https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-10-18-13-01/ffmpeg-n8.1.3-18-g46d899d7a0-win64-lgpl-shared-8.1.zip'; Sha256='EB9346BDB3312D51CE5BFCF78041FD17E3DB91F9FFF9446D0D758406B85D2A51'}
)
foreach($artifact in $artifacts){
    $archive=Join-Path $codecRoot ($artifact.Name+'.zip')
    if(-not(Test-Path -LiteralPath $archive)){
        if($Offline){throw "Missing offline archive: $archive"}
        $partial=$archive+'.part'
        Invoke-WebRequest -Uri $artifact.Url -OutFile $partial
        if((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ne $artifact.Sha256){throw "Codec archive checksum mismatch: $($artifact.Name)"}
        Move-Item -LiteralPath $partial -Destination $archive -Force
    }
    if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $artifact.Sha256){throw "Codec archive checksum mismatch: $($artifact.Name)"}
    $unpacked=Join-Path $codecRoot $artifact.Name
    $resolvedUnpacked=[IO.Path]::GetFullPath($unpacked)
    $codecPrefix=[IO.Path]::GetFullPath($codecRoot)+[IO.Path]::DirectorySeparatorChar
    if(-not $resolvedUnpacked.StartsWith($codecPrefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid codec extraction path'}
    if(Test-Path -LiteralPath $resolvedUnpacked){Remove-Item -LiteralPath $resolvedUnpacked -Recurse -Force}
    Expand-Archive -LiteralPath $archive -DestinationPath $unpacked -Force
}
$ffmpegSource=Get-Item -LiteralPath (Join-Path $codecRoot 'ffmpeg/ffmpeg-n8.1.3-18-g46d899d7a0-win64-lgpl-shared-8.1')
$selected=Join-Path $codecRoot 'ffmpeg-selected'
foreach($codecDirectory in @($selected,(Join-Path $codecRoot 'runtime'))){
    $resolvedDirectory=[IO.Path]::GetFullPath($codecDirectory)
    $codecPrefix=[IO.Path]::GetFullPath($codecRoot)+[IO.Path]::DirectorySeparatorChar
    if(-not $resolvedDirectory.StartsWith($codecPrefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid codec runtime path'}
    if(Test-Path -LiteralPath $resolvedDirectory){Remove-Item -LiteralPath $resolvedDirectory -Recurse -Force}
}
New-Item -ItemType Directory -Force -Path $selected | Out-Null
Copy-Item -Path (Join-Path $ffmpegSource.FullName '*') -Destination $selected -Recurse -Force
$runtime=Join-Path $codecRoot 'runtime'
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
$allowed=@('ffmpeg.exe','ffprobe.exe','avcodec-62.dll','avdevice-62.dll','avfilter-11.dll','avformat-62.dll','avutil-60.dll','swresample-6.dll','swscale-9.dll')
foreach($codecFile in $allowed){Copy-Item -LiteralPath (Join-Path $selected ('bin/'+$codecFile)) -Destination $runtime -Force}
Copy-Item -LiteralPath (Join-Path $codecRoot 'libraw/LibRaw-0.22.2/bin/libraw.dll') -Destination $runtime -Force
$licenses=Join-Path $runtime 'licenses'
New-Item -ItemType Directory -Force -Path $licenses | Out-Null
Copy-Item -LiteralPath (Join-Path $selected 'LICENSE.txt') -Destination (Join-Path $licenses 'FFmpeg-LGPL.txt') -Force
Copy-Item -LiteralPath (Join-Path $codecRoot 'libraw/LibRaw-0.22.2/LICENSE.LGPL') -Destination (Join-Path $licenses 'LibRaw-LGPL.txt') -Force
Copy-Item -LiteralPath (Join-Path $codecRoot 'libraw/LibRaw-0.22.2/COPYRIGHT') -Destination (Join-Path $licenses 'LibRaw-COPYRIGHT.txt') -Force
$artifacts | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runtime 'provenance.json') -Encoding utf8
& python (Join-Path $PSScriptRoot 'prune-runtime-notices.py') codecs
if($LASTEXITCODE -ne 0){throw 'Codec runtime notice archival failed'}
$checksums=[ordered]@{}
foreach($entry in (Get-ChildItem -LiteralPath $runtime -File -Recurse -Force | Sort-Object FullName)){
    $relative=[IO.Path]::GetRelativePath($runtime,$entry.FullName).Replace('\','/')
    if($relative -eq 'checksums.json'){continue}
    $checksums[$relative]=(Get-FileHash -LiteralPath $entry.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$checksums | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runtime 'checksums.json') -Encoding utf8
Write-Output "Bundled media components ready: $runtime"
