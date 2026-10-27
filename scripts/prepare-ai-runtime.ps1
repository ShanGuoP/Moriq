# 按固定修订和 SHA-256 准备 Qwen 运行组件，离线模式仅使用本地已验证归档。
param([switch]$Offline)
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$targetRoot=Join-Path $projectRoot 'target'
$runtime=Join-Path $targetRoot 'ai-runtime'
$cache=Join-Path $targetRoot 'downloads'
$archive=Join-Path $cache 'llama-b11869-bin-win-vulkan-x64.zip'
$sha='62d6fe2a8614d9b8a5c1e0484350fd6d91779661f84220617ad296672ef640a0'
$url='https://github.com/ggml-org/llama.cpp/releases/download/b11869/llama-b11869-bin-win-vulkan-x64.zip'
New-Item -ItemType Directory -Path $cache -Force | Out-Null
# 缺少缓存时下载到临时文件，核对内容后才发布为可复用归档。
if(-not(Test-Path -LiteralPath $archive)){
    if($Offline){throw 'Missing offline llama.cpp archive in target/downloads'}
    Invoke-WebRequest -Uri $url -OutFile ($archive+'.part')
    if((Get-FileHash -LiteralPath ($archive+'.part') -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sha){throw 'llama archive checksum mismatch'}
    Move-Item -LiteralPath ($archive+'.part') -Destination $archive -Force
}
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sha){throw 'llama archive checksum mismatch'}
# Only this generated application-owned runtime is replaced.
# 只替换 target 内的生成目录，并拒绝清理包含重解析点的运行目录。
$prefix=[IO.Path]::GetFullPath($targetRoot)+[IO.Path]::DirectorySeparatorChar
if(-not [IO.Path]::GetFullPath($runtime).StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid runtime cleanup path'}
if(Test-Path -LiteralPath $runtime){
    $entries=@(Get-Item -LiteralPath $runtime)+@(Get-ChildItem -LiteralPath $runtime -Recurse -Force)
    if($entries | Where-Object {$_.Attributes -band [IO.FileAttributes]::ReparsePoint}){throw 'AI runtime cannot contain reparse points'}
    Remove-Item -LiteralPath $runtime -Recurse -Force
}
New-Item -ItemType Directory -Path $runtime -Force | Out-Null
$unpacked=Join-Path $targetRoot ('llama-prepare-'+[Guid]::NewGuid().ToString('N'))
try {
    Expand-Archive -LiteralPath $archive -DestinationPath $unpacked
    $llama=Join-Path $runtime 'llama'
    New-Item -ItemType Directory -Path $llama | Out-Null
    $server=Get-ChildItem -LiteralPath $unpacked -Filter 'llama-server.exe' -File -Recurse | Select-Object -First 1
    if(-not $server){throw 'Missing llama server in archive'}
    $source=$server.DirectoryName
    $required=@('llama-server.exe','llama-server-impl.dll','llama-common.dll','llama.dll','mtmd.dll','ggml.dll','ggml-base.dll','ggml-vulkan.dll','ggml-cpu-x64.dll','libomp.dll')
    foreach($name in $required){Copy-Item -LiteralPath (Join-Path $source $name) -Destination $llama}
    Get-ChildItem -LiteralPath $source -Filter 'ggml-cpu-*.dll' -File | Copy-Item -Destination $llama -Force
    $notices=Join-Path $targetRoot 'package-reports/runtime-notices/ai/llama'
    New-Item -ItemType Directory -Path $notices -Force | Out-Null
    Get-ChildItem -LiteralPath $source -Filter 'LICENSE*' -File | Copy-Item -Destination $notices -Force
    # 分别生成 llama 子目录与完整 AI 目录的校验清单，供程序启动时验证。
    $llamaChecksums=[ordered]@{}
    foreach($file in (Get-ChildItem -LiteralPath $llama -File | Sort-Object Name)){
        $llamaChecksums[$file.Name]=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $llamaChecksums | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $llama 'checksums.json') -Encoding utf8
    Copy-Item -LiteralPath (Join-Path $projectRoot 'workers/ai/models.json') -Destination $runtime
    @{Runtime='llama.cpp';Version='b11869';Url=$url;Sha256=$sha;Backend='Vulkan';Models='Qwen3.5-4B; optional verified download'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runtime 'provenance.json') -Encoding utf8
    $checksums=[ordered]@{}
    foreach($file in (Get-ChildItem -LiteralPath $runtime -File -Recurse | Sort-Object FullName)){
        $relative=[IO.Path]::GetRelativePath($runtime,$file.FullName).Replace('\','/')
        $checksums[$relative]=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $checksums | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runtime 'checksums.json') -Encoding utf8
} finally {
    if(-not [IO.Path]::GetFullPath($unpacked).StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid preparation cleanup path'}
    if(Test-Path -LiteralPath $unpacked){Remove-Item -LiteralPath $unpacked -Recurse -Force}
}
Write-Output "Bundled Qwen-only AI runtime ready: $runtime"
