param([switch]$Offline)
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$runtime=Join-Path $projectRoot 'target/ai-runtime'
$source=Join-Path $projectRoot 'workers/ai'
. (Join-Path $PSScriptRoot 'ai-runtime-files.ps1')
New-Item -ItemType Directory -Path $runtime -Force | Out-Null
$archive=Join-Path $runtime 'node.zip'
$sha='edaca9bd58ec8e92037dac4e877d52f6b8f430b81c18b57e264b4e2fb111cd56'
$url='https://nodejs.org/dist/v24.16.0/node-v24.16.0-win-x64.zip'
if(-not(Test-Path -LiteralPath $archive)){
    if($Offline){throw 'Missing offline Node runtime archive'}
    Invoke-WebRequest -Uri $url -OutFile ($archive+'.part')
    if((Get-FileHash -LiteralPath ($archive+'.part') -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sha){throw 'Node archive checksum mismatch'}
    Move-Item -LiteralPath ($archive+'.part') -Destination $archive -Force
}
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sha){throw 'Node archive checksum mismatch'}
# This directory is generated. Retain only the shared shipping inventory and
# the verified offline Node cache; old local validation scripts are not runtime.
$runtimePrefix=[IO.Path]::GetFullPath($runtime)+[IO.Path]::DirectorySeparatorChar
if((Get-Item -LiteralPath $runtime).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'AI runtime cannot be a reparse directory'}
foreach($entry in (Get-ChildItem -LiteralPath $runtime -Force)){
    if($entry.Name -in ($aiRuntimeEntries+@('node.zip','node-unpacked'))){continue}
    $generatedPath=[IO.Path]::GetFullPath($entry.FullName)
    if(-not $generatedPath.StartsWith($runtimePrefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid generated runtime cleanup path'}
    Remove-Item -LiteralPath $generatedPath -Recurse -Force
}
Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $runtime 'node-unpacked') -Force
$node=Join-Path $runtime 'node-unpacked/node-v24.16.0-win-x64'
Copy-Item -LiteralPath (Join-Path $node 'node.exe') -Destination $runtime -Force
Copy-Item -LiteralPath (Join-Path $node 'LICENSE') -Destination (Join-Path $runtime 'Node-LICENSE.txt') -Force
foreach($file in @('package.json','package-lock.json','worker.mjs','faces.mjs','models.json')){Copy-Item -LiteralPath (Join-Path $source $file) -Destination $runtime -Force}
Push-Location -LiteralPath $runtime
try {
    # Use the pinned Node runtime, including for npm lifecycle subprocesses.
    $oldPath=$env:PATH
    $env:PATH=$node+[IO.Path]::PathSeparator+$oldPath
    $arguments=@((Join-Path $node 'node_modules/npm/bin/npm-cli.js'),'ci','--omit=dev','--ignore-scripts','--no-fund')
    if($Offline){$arguments+='--offline'}
    & (Join-Path $node 'node.exe') @arguments
    if($LASTEXITCODE -ne 0){throw 'Pinned AI runtime installation failed'}
} finally {$env:PATH=$oldPath;Pop-Location}
Copy-Item -LiteralPath (Join-Path $projectRoot 'assets/licenses/ONNX-Runtime-MIT.txt') -Destination (Join-Path $runtime 'node_modules/onnxruntime-node/LICENSE') -Force
# The pinned transformers.node.mjs exports stub out onnxruntime-web. Keep the
# upstream lock intact while removing only these unused browser-only packages.
foreach($unusedPackage in @('onnxruntime-web','guid-typescript',
    'onnxruntime-node/bin/napi-v3/darwin','onnxruntime-node/bin/napi-v3/linux','onnxruntime-node/bin/napi-v3/win32/arm64')){
    $unusedPath=[IO.Path]::GetFullPath((Join-Path $runtime ('node_modules/'+$unusedPackage)))
    $modulesPrefix=[IO.Path]::GetFullPath((Join-Path $runtime 'node_modules'))+[IO.Path]::DirectorySeparatorChar
    if(-not $unusedPath.StartsWith($modulesPrefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid runtime pruning path'}
    if(Test-Path -LiteralPath $unusedPath){Remove-Item -LiteralPath $unusedPath -Recurse -Force}
}
@{NodeVersion='24.16.0';Url=$url;Sha256=$sha;TransformersJS='3.8.1';OnnxRuntime='1.21.0';Models='Downloaded only when requested in Moriq Space'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runtime 'provenance.json') -Encoding utf8
& python (Join-Path $PSScriptRoot 'prune-runtime-notices.py') ai
if($LASTEXITCODE -ne 0){throw 'AI runtime notice archival failed'}
$checksums=[ordered]@{}
foreach($entry in (Get-ChildItem -LiteralPath $runtime -File -Recurse -Force | Sort-Object FullName)){
    $relative=[IO.Path]::GetRelativePath($runtime,$entry.FullName).Replace('\','/')
    if($relative -eq 'checksums.json' -or $relative -eq 'node.zip' -or $relative.StartsWith('node-unpacked/')){continue}
    $checksums[$relative]=(Get-FileHash -LiteralPath $entry.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$checksums | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $runtime 'checksums.json') -Encoding utf8
Write-Output "Bundled local AI runtime ready: $runtime"
