param([switch]$Offline)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'verified-download.ps1')
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$cache=Join-Path $root 'target/downloads'
$runtime=Join-Path $root 'target/ai-runtime/faces'
$models=Join-Path $root 'target/face-models'
New-Item -ItemType Directory -Path $cache,$models,$runtime -Force | Out-Null
$archive=Join-Path $cache 'opencv-4.12.0-windows.exe'
$url='https://github.com/opencv/opencv/releases/download/4.12.0/opencv-4.12.0-windows.exe'
$archiveHash='b753b14d880b9bc8d89d6acd3b665c040baec0211078435432fcae117db707af'
Get-MoriqVerifiedDownload -Uri $url -Destination $archive -Sha256 $archiveHash -Offline:$Offline
$sdk=Join-Path $root 'target/opencv-sdk'
if(-not(Test-Path (Join-Path $sdk 'opencv/build/include/opencv2/core.hpp'))) {
    & $archive -y "-o$sdk"
    if($LASTEXITCODE -ne 0){throw 'OpenCV SDK extraction failed'}
}
$commit='48b10fa505aeb51fbd127fc60a5c660040e823b7'
$entries=@(
    @{Name='yunet.onnx';Path='face_detection_yunet/face_detection_yunet_2023mar.onnx';Hash='8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4'},
    @{Name='sface.onnx';Path='face_recognition_sface/face_recognition_sface_2021dec.onnx';Hash='0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79'}
)
foreach($entry in $entries) {
    $file=Join-Path $models $entry.Name
    Get-MoriqVerifiedDownload -Uri "https://media.githubusercontent.com/media/opencv/opencv_zoo/$commit/models/$($entry.Path)" -Destination $file -Sha256 $entry.Hash -Offline:$Offline
    Copy-Item -LiteralPath $file -Destination $runtime -Force
}
Copy-Item (Join-Path $sdk 'opencv/build/x64/vc16/bin/opencv_world4120.dll') $runtime -Force
foreach($name in @('msvcp140.dll','msvcp140_1.dll','msvcp140_2.dll','vcruntime140.dll','vcruntime140_1.dll','concrt140.dll')) {
    Copy-Item -LiteralPath (Join-Path $env:WINDIR "System32/$name") -Destination $runtime -Force
}
$credits=Join-Path $models 'CREDITS.txt'
if(-not(Test-Path $credits)) {
    if($Offline){throw 'Missing face runtime licenses'}
    $text="OpenCV 4.12.0 (Apache-2.0)`n"+(Get-Content (Join-Path $sdk 'opencv/build/LICENSE') -Raw)
    foreach($model in @('face_detection_yunet','face_recognition_sface')) {
        $license=Invoke-WebRequest "https://raw.githubusercontent.com/opencv/opencv_zoo/$commit/models/$model/LICENSE"
        $text+="`n$model`n"+$license.Content
    }
    $text+="`nMicrosoft Visual C++ Redistributable DLLs are distributed under the Visual Studio distributable-code terms. https://visualstudio.microsoft.com/license-terms/`n"
    $text | Set-Content $credits -Encoding utf8
}
Copy-Item $credits $runtime -Force
@{Runtime='OpenCV';Version='4.12.0';Archive=$url;Sha256=$archiveHash;ModelCommit=$commit;Backend='CPU';Threads=2} | ConvertTo-Json | Set-Content (Join-Path $runtime 'provenance.json') -Encoding utf8
