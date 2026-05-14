$ErrorActionPreference = "Stop"
$Repo = "vmargb/myoso"
$BinName = "myoso.exe"
$Target = "x86_64-pc-windows-msvc"

Write-Host "Fetching latest release..."
$Release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$Tag = $Release.tag_name

if (-not $Tag) {
    Write-Error "Failed to fetch latest version"
}

$Artifact = "myoso-$Target.zip"
$Url = "https://github.com/$Repo/releases/download/$Tag/$Artifact"

$TempDir = New-Item -ItemType Directory -Path ([System.IO.Path]::GetTempPath()) -Name ("myoso_" + [guid]::NewGuid())
Set-Location $TempDir

Write-Host "Downloading $Url..."
Invoke-WebRequest $Url -OutFile $Artifact

Write-Host "Extracting..."
Expand-Archive $Artifact -DestinationPath .

# install dir
$InstallDir = "$env:USERPROFILE\.myoso\bin"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$DestPath = "$InstallDir\$BinName"
$OldPath = "$DestPath.old"

# *** WINDOWS FILE LOCK BYPASS ***
# cannot overwrite a running executable, but we can rename it
if (Test-Path $DestPath) {
    # delete any lingering .old file from a previous update
    if (Test-Path $OldPath) {
        Remove-Item $OldPath -Force -ErrorAction SilentlyContinue
    }
    # rename the currently running binary out of the way
    Rename-Item -Path $DestPath -NewName "$BinName.old" -Force
}

Move-Item "$BinName" $DestPath -Force
Write-Host "Installed to $InstallDir"

# add to PATH if missing
$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", "User")

if ($CurrentPath -notlike "*$InstallDir*") {
    Write-Host "Adding to PATH..."
    [Environment]::SetEnvironmentVariable("PATH", "$CurrentPath;$InstallDir", "User")
    Write-Host "Added to PATH. Restart your terminal."
} else {
    Write-Host "Already in PATH"
}

Write-Host ""
Write-Host "Run with: myoso"