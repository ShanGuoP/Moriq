$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'verified-download.ps1')
$testParent=Join-Path ([IO.Path]::GetTempPath()) 'Moriq-verified-download-tests'
$testRoot=[IO.Path]::GetFullPath((Join-Path $testParent ([Guid]::NewGuid().ToString('N'))))
$testPrefix=[IO.Path]::GetFullPath($testParent)+[IO.Path]::DirectorySeparatorChar
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
$downloadProbe=@{Calls=0;Mode='valid'}
# No network: simulate interrupted connections and incorrect responses at the download boundary.
function Invoke-WebRequest {
    param($Uri,$OutFile)
    $downloadProbe.Calls++
    if($downloadProbe.Mode -eq 'interrupt'){
        [IO.File]::WriteAllText($OutFile,'partial')
        throw 'simulated connection interruption'
    }
    [IO.File]::WriteAllText($OutFile,$downloadProbe.Mode)
}
function Assert-DownloadProbe {
    param([bool]$Condition,[string]$Message)
    if(-not $Condition){throw $Message}
}
function Assert-DownloadFailure {
    param([scriptblock]$Operation)
    $failed=$false
    try { & $Operation } catch { $failed=$true }
    Assert-DownloadProbe $failed 'Expected download failure'
}
try {
    $expected=Join-Path $testRoot 'expected'
    [IO.File]::WriteAllText($expected,'valid')
    $hash=(Get-FileHash -LiteralPath $expected -Algorithm SHA256).Hash
    $cache=Join-Path $testRoot 'cached.bin'
    $arguments=@{Uri='https://example.invalid/fixture';Destination=$cache;Sha256=$hash}

    $downloadProbe.Mode='interrupt'
    Assert-DownloadFailure { Get-MoriqVerifiedDownload @arguments }
    Assert-DownloadProbe (-not(Test-Path -LiteralPath $cache)) 'Interrupted download published a cache file'
    Assert-DownloadProbe (-not(Test-Path -LiteralPath ($cache+'.part'))) 'Interrupted download retained a partial file'

    $downloadProbe.Mode='valid'
    Get-MoriqVerifiedDownload @arguments
    Assert-DownloadProbe ($downloadProbe.Calls -eq 2) 'Retry did not download again'
    Get-MoriqVerifiedDownload @arguments -Offline
    Assert-DownloadProbe ($downloadProbe.Calls -eq 2) 'Offline cache hit accessed the network'

    [IO.File]::WriteAllText($cache,'old partial download')
    Assert-DownloadFailure { Get-MoriqVerifiedDownload @arguments -Offline }
    Assert-DownloadProbe ($downloadProbe.Calls -eq 2) 'Invalid offline cache accessed the network'
    Assert-DownloadProbe ([IO.File]::ReadAllText($cache) -eq 'old partial download') 'Offline check modified the cache'

    $downloadProbe.Mode='corrupt'
    Assert-DownloadFailure { Get-MoriqVerifiedDownload @arguments }
    Assert-DownloadProbe ([IO.File]::ReadAllText($cache) -eq 'old partial download') 'Incorrect download replaced the cache'
    Assert-DownloadProbe (-not(Test-Path -LiteralPath ($cache+'.part'))) 'Incorrect download retained a partial file'

    $downloadProbe.Mode='valid'
    [IO.File]::WriteAllText($cache+'.part','stale partial download')
    Get-MoriqVerifiedDownload @arguments
    Assert-DownloadProbe ((Get-FileHash -LiteralPath $cache -Algorithm SHA256).Hash -eq $hash) 'Invalid cache could not recover'
    Assert-DownloadProbe ($downloadProbe.Calls -eq 4) 'Invalid cache was not retried'
    Write-Output 'Verified download regressions passed: interruption, retry, offline cache, checksum rejection and recovery.'
} finally {
    if(-not $testRoot.StartsWith($testPrefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid download test cleanup path'}
    if(Test-Path -LiteralPath $testRoot){Remove-Item -LiteralPath $testRoot -Recurse -Force}
}
