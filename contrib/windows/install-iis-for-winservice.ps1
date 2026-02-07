Param(
    [String]$SiteName="Default Web Site"
)
$ErrorActionPreference = 'Stop'

$homedir = [Environment]::GetEnvironmentVariable('IMAGESHARE_HOME', [EnvironmentVariableTarget]::Machine)
if ($homedir -eq $null) {
    Write-Host "Error: You didn't install imageshare-rs"
    return
}

$appcmd = "C:\Windows\System32\inetsrv\appcmd.exe"

"$homedir\i", "$homedir\p" | % {
    New-Item -Type Directory -Force -Path $_
    # add IIS access for AppPool Identites (IIS_IUSERS) and Anonymous user (IUSR)
    $acl = Get-Acl $_
    $read = 'Read', 'ContainerInherit,ObjectInherit', 'None', 'Allow'
    # $write = 'Read, Write', 'ContainerInherit,ObjectInherit', 'None', 'Allow'
    $aclrules =
        (New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, 'BUILTIN\IIS_IUSRS' + $read)),
        (New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, 'NT AUTHORITY\IUSR' + $read))
    foreach ($rule in $aclrules) {
        $acl.AddAccessRule($rule)
    }
    $acl | Set-Acl -Path $_
    $base = (Get-Item $_).BaseName
    & $appcmd add vdir "/app.name:$SiteName/" "/path:/$base" "/physicalPath:$_"
}

# set charset=utf8 for plaintext content
& $appcmd set config "$SiteName/p" -section:system.webServer/staticContent "/[fileExtension='.txt'].mimeType:text/plain; charset=utf8"
# make sure ARR sends Host unchanged. seems like Edge does not send Sec-Fetch-Site used over non https? dunno..
& $appcmd set config -section:system.webServer/proxy -preserveHostHeader:true /commit:apphost
