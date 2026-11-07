# Publish cached downloads only after their pinned checksum has been verified.
function Get-MoriqVerifiedDownload {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Uri,
        [Parameter(Mandatory)][string]$Destination,
        [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$Sha256,
        [switch]$Offline
    )
    if((Test-Path -LiteralPath $Destination -PathType Leaf) -and
        (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash -eq $Sha256){return}
    if($Offline){throw "Missing or corrupt offline download: $Destination"}
    $partial=$Destination+'.part'
    try {
        Invoke-WebRequest -Uri $Uri -OutFile $partial
        if((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ne $Sha256){
            throw "Download checksum mismatch: $Destination"
        }
        Move-Item -LiteralPath $partial -Destination $Destination -Force
    } finally {
        if(Test-Path -LiteralPath $partial){Remove-Item -LiteralPath $partial -Force}
    }
}
