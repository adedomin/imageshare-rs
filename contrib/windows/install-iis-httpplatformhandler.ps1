Param(
    [String]$Bin='.\target\release\imageshare-rs.exe',
    [String]$LinkPrefix="http://localhost",
    [String]$SiteName="Default Web Site"
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

# set config and state home.
$IMAGESHARE_HOME = "$env:ProgramData\imageshare-rs"
New-Item -Type Directory -Force -Path $IMAGESHARE_HOME
[System.Environment]::SetEnvironmentVariable('IMAGESHARE_HOME', $IMAGESHARE_HOME, [System.EnvironmentVariableTarget]::Machine)
# create new acl
$acl = New-Object System.Security.AccessControl.DirectorySecurity
# first disables inheritance (?), second removes all existing inherited access.
$acl.SetAccessRuleProtection($true, $false)
$owner = New-Object System.Security.Principal.NTAccount -ArgumentList "BUILTIN\IIS_IUSRS"
$acl.SetOwner($owner)
$fullcontrol = 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'
$aclrules =
    (New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, 'NT AUTHORITY\SYSTEM' + $fullcontrol)),
    (New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, 'BUILTIN\Administrators' + $fullcontrol)),
    (New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, 'BUILTIN\IIS_IUSRS' + $fullcontrol))
foreach ($rule in $aclrules) {
    $acl.AddAccessRule($rule)
}
$acl | Set-Acl -Path $IMAGESHARE_HOME

try {
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
    # bind to a given socket addr, this installer is for HttpPlatformHandler, so we use the %PORT% bind
    # note that rt-dir: and unix: protocol do not work on Windows.
    bind = "127.0.0.1:%PORT%"
} |
ConvertTo-Json |
Out-File `
    -FilePath "$IMAGESHARE_HOME\config.json" `
    -Encoding ASCII `
    -NoClobber
} catch {}
# Note that Out-File in PowerShell 5.1 does not have UTF8 without a BOM.

$appcmd = "C:\Windows\System32\inetsrv\appcmd.exe"

# set up vdirs
# this must be done at the "apphost.config" level, so we use appcmd.
"$IMAGESHARE_HOME\i", "$IMAGESHARE_HOME\p" | % {
    New-Item -Type Directory -Force -Path $_
    # add IIS access for AppPool Identites (IIS_IUSERS) and Anonymous user (IUSR)
    $acl = Get-Acl $_
    $read = 'Read', 'ContainerInherit,ObjectInherit', 'None', 'Allow'
    $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList (, 'NT AUTHORITY\IUSR' + $read)))
    $acl | Set-Acl -Path $_
    $base = (Get-Item $_).BaseName
    & $appcmd add vdir "/app.name:$SiteName/" "/path:/$base" "/physicalPath:$_"
}

# set up our web.config for hosting.
& $appcmd unlock config -section:system.webServer/handlers
& $appcmd unlock config -section:system.webServer/security/requestFiltering
$webconf = [Environment]::ExpandEnvironmentVariables((& $appcmd list vdir /app.name:"$SiteName/" /text:physicalPath)[0])
@"
<?xml version="1.0" encoding="utf-8"?>
<!-- ImageShare-rs IIS Configuration. -->
<configuration>
  <!--
    Requires:
      HttpPlatformHandler: https://www.iis.net/downloads/microsoft/httpplatformhandler
  -->
  <location path="" inheritInChildApplications="false">
    <system.webServer>
      <handlers>
        <clear />
        <add name="ImageShareHandler" path="*" verb="*" modules="httpPlatformHandler" resourceType="Unspecified" />
      </handlers>
      <httpPlatform
        processPath="$IMAGESHARE_EXE"
        stdoutLogEnabled="true"
        stdoutLogFile="%IMAGESHARE_HOME%\imageshare-rs-iis"
        startupTimeLimit="30"
        processesPerApplication="1"
      >
      </httpPlatform>
    </system.webServer>
  </location>
  <!--
    Requires:
      Windows Feature:
      * Internet Information Services
        * World Wide Web Services
          * Common HTTP Features
            * Static Content
  -->
  <location path="p" inheritInChildApplications="false">
    <system.webServer>
      <handlers>
        <clear />
        <add name="PasteFiles" path="*.txt" verb="*" modules="StaticFileModule" resourceType="Either" />
      </handlers>
      <staticContent>
        <clear />
        <!-- All pastes are valid UTF-8 -->
        <mimeMap fileExtension=".txt" mimeType="text/plain; charset=utf8" />
      </staticContent>
    </system.webServer>
  </location>
  <location path="i" inheritInChildApplications="false">
    <system.webServer>
      <handlers>
        <clear />
        <add name="ImageFiles" path="*" verb="*" modules="StaticFileModule" resourceType="Either" />
      </handlers>
    </system.webServer>
  </location>
  <system.webServer>
    <!--
      Requires:
        Windows Feature:
        * Internet Information Services
          * World Wide Web Services
            * Security
              * Request Filtering
    -->
    <security>
      <requestFiltering>
        <!-- SET YOUR MAX ALLOWED SIZE HERE -->
        <requestLimits maxAllowedContentLength="$(10MB)" />
      </requestFiltering>
    </security>
  </system.webServer>
</configuration>
"@ | Out-File -FilePath "$webconf\web.config" -Encoding ASCII
