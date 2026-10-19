param([switch]$Offline)
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$runtime=Join-Path $projectRoot 'target/ai-runtime'
$source=Join-Path $projectRoot 'workers/ai'
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
    $arguments=@((Join-Path $node 'node_modules/npm/bin/npm-cli.js'),'ci','--omit=dev','--no-audit','--no-fund')
    if($Offline){$arguments+='--offline'}
    & (Join-Path $node 'node.exe') @arguments
    if($LASTEXITCODE -ne 0){throw 'Pinned AI runtime installation failed'}
} finally {$env:PATH=$oldPath;Pop-Location}
@{NodeVersion='24.16.0';Url=$url;Sha256=$sha;TransformersJS='3.8.1';OnnxRuntime='1.21.0';Models='Downloaded only when requested in MORIQ'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runtime 'provenance.json') -Encoding utf8
Write-Output "Bundled local AI runtime ready: $runtime"
