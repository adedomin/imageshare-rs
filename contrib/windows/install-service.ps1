Param(
    [String]$Bin='.\target\release\imageshare-rs.exe',
    [String]$ServiceAccount='NT Service\ImageShare-rs',
    [String]$LinkPrefix="http://localhost:8146",
    [bool]$AddToPath=$false
)
$ErrorActionPreference = 'Stop'

# install binary
$IMAGESHARE_BIN = "$env:ProgramFiles\imageshare-rs\bin"
$IMAGESHARE_EXE = "$IMAGESHARE_BIN\imageshare-rs.exe"
New-Item -Type Directory -Force -Path "$IMAGESHARE_BIN"
Copy-Item -Path $Bin -Destination "$IMAGESHARE_EXE"
if ($AddToPath) {
    $mpath = [System.Environment]::GetEnvironmentVariable('PATH', [System.EnvironmentVariableTarget]::Machine)
    $mpath += ";$IMAGESHARE_BIN"
    [System.Environment]::SetEnvironmentVariable('PATH', $mpath, [System.EnvironmentVariableTarget]::Machine)
}

# create service
# New-Service demands a password for -Credential, which we do not have.
# We need to create the service first so the Virtual Account (if using the default "NT Service\*" param) exists.
& cmd.exe /c (
    'sc.exe create ImageShare-rs binPath= """{0}""" obj= "{1}" start= auto' `
    -f $IMAGESHARE_EXE, $ServiceAccount
)
& sc.exe description ImageShare-rs 'ImageShare Web Application'

# set config and state home.
$IMAGESHARE_HOME = "$env:ProgramData\imageshare-rs"
New-Item -Type Directory -Force -Path $IMAGESHARE_HOME
[System.Environment]::SetEnvironmentVariable('IMAGESHARE_HOME', $IMAGESHARE_HOME, [System.EnvironmentVariableTarget]::Machine)
# create new acl
$acl = New-Object System.Security.AccessControl.DirectorySecurity
# first disables inheritance (?), second removes all existing inherited access.
$acl.SetAccessRuleProtection($true, $false)
$owner = New-Object System.Security.Principal.NTAccount -ArgumentList $ServiceAccount
$acl.SetOwner($owner)
$aclrules = 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'
$aclrules =
    (New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, $ServiceAccount + $aclrules)),
    (New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, 'NT AUTHORITY\SYSTEM' + $aclrules)),
    (New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, 'BUILTIN\Administrators' + $aclrules))
foreach ($rule in $aclrules) {
    $acl.AddAccessRule($rule)
}
$acl | Set-Acl -Path $IMAGESHARE_HOME

@{
    image = @{
        siz = 10MB
        cnt = 100
        # you should probably just use the default state home, which will be %IMAGESHARE_HOME%/i
        # dir = "$IMAGESHARE_HOME/i"
    }
    paste = @{
        # NOTE: pastes are fully buffered in memory. 64Ki should be big enough.
        #       in the (near) future, this may change.
        siz = 64KB
        cnt = 10000
        # you should probably just use the default state home, which will be %IMAGESHARE_HOME%/i
        # dir = "$IMAGESHARE_HOME/p"
    }
    # used for limint how many uploads from one user.
    ratelim = @{
        # how many seconds a user has to wait once they exhausted their burst.
        secs = 30
        # number of pictures they can upload before ratelim
        burst = 3
        # trust X-Real-IP header from upstream reverse proxy for ratelimiting decisions.
        trust_headers = $true
        # max number of slots for ratelimiting; IPs are hashed, no collision resistance.
        # IPv6 are truncated to 64bit numbers.
        # 16,384 is about 128KiB of state.
        bucket_size = 16384
    }
    # this is the url prefix that is used in generating urls for users.
    link_prefix = $LinkPrefix
    # bind to a given socket addr.
    # note that rt-dir: and unix: protocol do not work on Windows.
    bind = "[::1]:8146"
    # %PORT% --CASE SENSITIVE-- expands to the value of %HTTP_PLATFORM_PORT% or %FUNCTIONS_CUSTOMHANDLER_PORT%
    # bind = "[::1]:%PORT%"
} |
ConvertTo-Json |
Out-File -FilePath "$IMAGESHARE_HOME\config.json" `
    -Encoding ASCII `
    -NoClobber `
    -ErrorAction 'SilentlyContinue'
# Note that Out-File in PowerShell 5.1 does not have UTF8 without a BOM.

# Firewall
# only need this for direct listen, not really recommended.
New-NetFirewallRule `
    -DisplayName 'Allow Imageshare Web Server' `
    -Program $IMAGESHARE_EXE `
    -Direction Inbound -Action Allow
