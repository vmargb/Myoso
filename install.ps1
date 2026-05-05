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

Move-Item "$BinName" "$InstallDir\$BinName" -Force

Write-Host "Installed to $InstallDir"

# add to PATH if missing
$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", "User")

if ($CurrentPath -notlike "*$InstallDir*") {
    Write-Host "Adding to PATH..."
    [Environment]::SetEnvironmentVariable(
        "PATH",
        "$CurrentPath;$InstallDir",
        "User"
    )
    Write-Host "Restart your terminal to use 'myoso'"
} else {
    Write-Host "Already in PATH"
}

Write-Host ""
Write-Host "Run with: myoso"