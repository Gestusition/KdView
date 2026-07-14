[CmdletBinding()]
param(
    [string]$Port,
    [string]$TargetIp
)

$ErrorActionPreference = 'Stop'
$provision = Join-Path $PSScriptRoot '..\firmware\esp32-csi-node\provision.py'

if (-not $Port) {
    $usbPorts = @(
        Get-PnpDevice -PresentOnly -Class Ports -ErrorAction SilentlyContinue |
            Where-Object InstanceId -Like 'USB\*' |
            ForEach-Object {
                if ($_.FriendlyName -match '\((COM\d+)\)') {
                    [pscustomobject]@{
                        Port = $Matches[1]
                        Name = $_.FriendlyName
                    }
                }
            }
    )

    if ($usbPorts.Count -eq 1) {
        $Port = $usbPorts[0].Port
        Write-Host "ESP32 seri portu otomatik bulundu: $Port ($($usbPorts[0].Name))"
    }
    elseif ($usbPorts.Count -gt 1) {
        Write-Host 'Birden fazla USB seri portu bulundu:'
        for ($index = 0; $index -lt $usbPorts.Count; $index++) {
            Write-Host "  $($index + 1)) $($usbPorts[$index].Name)"
        }

        $selection = Read-Host "ESP32 portunun numarasi"
        $selectedIndex = 0
        if (-not [int]::TryParse($selection, [ref]$selectedIndex) -or
            $selectedIndex -lt 1 -or $selectedIndex -gt $usbPorts.Count) {
            throw 'Gecersiz seri port secimi.'
        }
        $Port = $usbPorts[$selectedIndex - 1].Port
    }
    else {
        $available = [IO.Ports.SerialPort]::GetPortNames() -join ', '
        throw "USB seri port bulunamadi. ESP32'yi baglayip tekrar deneyin. Gorunen portlar: $available"
    }
}

$availablePorts = [IO.Ports.SerialPort]::GetPortNames()
if ($Port -notin $availablePorts) {
    throw "Seri port $Port bulunamadi. Gorunen portlar: $($availablePorts -join ', ')"
}

if (-not $TargetIp) {
    $TargetIp = (Get-NetIPAddress -InterfaceAlias 'Wi-Fi' -AddressFamily IPv4 |
        Where-Object IPAddress -NotLike '169.254.*' |
        Select-Object -First 1).IPAddress
}

if (-not $TargetIp) {
    throw "Wi-Fi IPv4 adresi bulunamadi. Once bilgisayari Wi-Fi agina baglayin."
}

$ssid = Read-Host 'Wi-Fi ag adi (SSID)'
if ([string]::IsNullOrWhiteSpace($ssid)) {
    throw 'Wi-Fi ag adi bos olamaz.'
}
$securePassword = Read-Host 'Wi-Fi sifresi' -AsSecureString
$passwordPtr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)

try {
    $password = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordPtr)

    & python -X utf8 $provision `
        --port $Port `
        --chip esp32s3 `
        --ssid $ssid `
        "--password=$password" `
        --target-ip $TargetIp `
        --target-port 5005

    if ($LASTEXITCODE -ne 0) {
        throw "Wi-Fi ayarlari yazilamadi (kod: $LASTEXITCODE)."
    }

    Write-Host "Tamam. ESP32 $ssid agina baglanacak; CSI hedefi $TargetIp`:5005."
}
finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordPtr)
    $password = $null
}
