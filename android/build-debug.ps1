$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$jdk = Get-ChildItem (Join-Path $repoRoot "tools\jdk17") -Directory | Select-Object -First 1
$gradle = Get-ChildItem (Join-Path $repoRoot "tools\gradle") -Directory | Select-Object -First 1

if ($null -eq $jdk) {
    throw "JDK not found under tools\jdk17"
}
if ($null -eq $gradle) {
    throw "Gradle not found under tools\gradle"
}

$env:JAVA_HOME = $jdk.FullName
$env:ANDROID_HOME = Join-Path $repoRoot "tools\android-sdk"
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
$env:Path = "$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;$($gradle.FullName)\bin;$env:Path"

Push-Location $PSScriptRoot
try {
    & "$($gradle.FullName)\bin\gradle.bat" :app:assembleDebug
}
finally {
    Pop-Location
}
