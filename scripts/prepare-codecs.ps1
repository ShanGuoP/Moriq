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
    Expand-Archive -LiteralPath $archive -DestinationPath $unpacked -Force
}
$ffmpegSource=Get-Item -LiteralPath (Join-Path $codecRoot 'ffmpeg/ffmpeg-n8.1.3-18-g46d899d7a0-win64-lgpl-shared-8.1')
$selected=Join-Path $codecRoot 'ffmpeg-selected'
New-Item -ItemType Directory -Force -Path $selected | Out-Null
Copy-Item -Path (Join-Path $ffmpegSource.FullName '*') -Destination $selected -Recurse -Force
$runtime=Join-Path $codecRoot 'runtime'
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
Copy-Item -Path (Join-Path $selected 'bin/*') -Destination $runtime -Force
$licenses=Join-Path $runtime 'licenses'
New-Item -ItemType Directory -Force -Path $licenses | Out-Null
Copy-Item -LiteralPath (Join-Path $selected 'LICENSE.txt') -Destination (Join-Path $licenses 'FFmpeg-LGPL.txt') -Force
Copy-Item -LiteralPath (Join-Path $codecRoot 'libraw/LibRaw-0.22.2/LICENSE.LGPL') -Destination (Join-Path $licenses 'LibRaw-LGPL.txt') -Force
Copy-Item -LiteralPath (Join-Path $codecRoot 'libraw/LibRaw-0.22.2/COPYRIGHT') -Destination (Join-Path $licenses 'LibRaw-COPYRIGHT.txt') -Force
$artifacts | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runtime 'provenance.json') -Encoding utf8
$checksums=@{}
Get-ChildItem -LiteralPath $runtime -File | Where-Object Extension -In '.dll','.exe' | ForEach-Object {$checksums[$_.Name]=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
$checksums | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runtime 'checksums.json') -Encoding utf8
Write-Output "Bundled media components ready: $runtime"
