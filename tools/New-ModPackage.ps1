[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Source,
    [Parameter(Mandatory)][string]$Id,
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$BaseVersion,
    [Parameter(Mandatory)][string]$Description,
    [Parameter(Mandatory)][string[]]$Files,
    [Parameter(Mandatory)][string[]]$Authors,
    [Parameter(Mandatory)][ValidateSet('runtime', 'addon')][string]$Kind,
    [string]$ReleaseNotes = '',
    [string]$Output,
    [string]$GitHubRepository,
    [string]$CatalogEntryPath,
    [switch]$ReleaseAssets
)

$ErrorActionPreference = 'Stop'
$token = '^[a-zA-Z0-9](?:[a-zA-Z0-9._-]{0,62}[a-zA-Z0-9])?$'
$reserved = '^(?:con|prn|aux|nul|com[0-9]|lpt[0-9])(?:\.|$)'
foreach ($value in @($Id, $Version)) {
    if ($value -cne $value.ToLowerInvariant() -or $value -notmatch $token -or $value.Contains('..') -or $value -match $reserved) {
        throw 'Use a simple package identifier and version.'
    }
}
if ($Id -eq 'official-cod2x' -or $BaseVersion -notmatch '^\d+\.\d+\.\d+\.\d+$') {
    throw 'Use a custom ID and a four-part official CoD2x base version.'
}
if ($GitHubRepository -and $GitHubRepository -notmatch '^[a-zA-Z0-9._-]+/[a-zA-Z0-9._-]+$') {
    throw 'GitHubRepository must use owner/repository format.'
}
if ($ReleaseAssets -and $GitHubRepository) {
    throw 'ReleaseAssets uses each mod repository directly. Omit GitHubRepository.'
}
if (-not $Output) { $Output = if ($ReleaseAssets) { 'mod.zip' } else { "$Id-v$Version.zip" } }
$destination = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Output)
if ([IO.Path]::GetExtension($destination) -ne '.zip' -or (Test-Path -LiteralPath $destination)) {
    throw 'Choose a new output ZIP file.'
}
if ($GitHubRepository -and [IO.Path]::GetFileName($destination) -notmatch '^[a-zA-Z0-9._-]+\.zip$') {
    throw 'Use a simple GitHub asset filename.'
}

if ($ReleaseAssets) {
    if ([IO.Path]::GetFileName($destination) -cne 'mod.zip') { throw 'Name the release archive mod.zip.' }
    if (-not $CatalogEntryPath) { $CatalogEntryPath = Join-Path ([IO.Path]::GetDirectoryName($destination)) 'mod.json' }
}
if ($CatalogEntryPath) {
    if (-not $GitHubRepository -and -not $ReleaseAssets) { throw 'CatalogEntryPath requires ReleaseAssets or GitHubRepository.' }
    $entryPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($CatalogEntryPath)
    if ($ReleaseAssets -and [IO.Path]::GetFileName($entryPath) -cne 'mod.json') { throw 'Name the release descriptor mod.json.' }
    if ($entryPath -eq $destination -or [IO.Path]::GetExtension($entryPath) -ne '.json' -or (Test-Path -LiteralPath $entryPath)) {
        throw 'Choose a new catalog entry JSON file.'
    }
}

$root = (Resolve-Path -LiteralPath $Source).ProviderPath
$entries = [ordered]@{}
$size = 0L
foreach ($name in $Files) {
    $path = $name.Replace('\', '/')
    $archiveName = if ($path.StartsWith('main/') -and $path.EndsWith('.iwd')) { $path.Substring(5, $path.Length - 9) }
    if ($path -cne 'mss32.dll' -and ($archiveName -notmatch $token -or $archiveName -cne $archiveName.ToLowerInvariant() -or $archiveName.Contains('..') -or $archiveName -match $reserved)) {
        throw "Unsupported package path $path"
    }
    if ($archiveName -match '^(?:iw_(?:0[0-9]|1[0-5])|iw_cod2x_01|localized_.+_iw[0-9]{2})$') {
        throw "Base game and CoD2x archives are managed by the official installation: $path"
    }
    if ($entries.Contains($path)) { throw "Duplicate path $path" }
    $file = Get-Item -LiteralPath (Join-Path $root $path)
    if ($file.PSIsContainer -or $file.Attributes.HasFlag([IO.FileAttributes]::ReparsePoint)) {
        throw "Expected a regular file for $path"
    }
    $size += $file.Length
    $entries[$path] = $file.FullName
}
if ($entries.Count -gt 64 -or $size -gt 512MB -or (($Kind -eq 'runtime') -ne $entries.Contains('mss32.dll'))) {
    throw 'The package exceeds its size or file limit, or does not contain the files required by its package type.'
}

$manifest = [ordered]@{
    id = $Id
    name = $Name
    version = $Version
    description = $Description
    authors = @($Authors)
    releaseNotes = $ReleaseNotes
    kind = $Kind
    baseVersion = $BaseVersion
    files = @(foreach ($path in $entries.Keys) {
        [ordered]@{ path = $path; sha256 = (Get-FileHash -LiteralPath $entries[$path] -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
}
$manifestJson = $manifest | ConvertTo-Json -Depth 8
if ([Text.Encoding]::UTF8.GetByteCount($manifestJson) -gt 64KB) { throw 'The manifest exceeds 64 KB.' }

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$temporary = Join-Path ([IO.Path]::GetDirectoryName($destination)) ([Guid]::NewGuid().ToString('N') + '.zip')
try {
    $zip = [IO.Compression.ZipFile]::Open($temporary, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $writer = [IO.StreamWriter]::new($zip.CreateEntry('manifest.json').Open(), [Text.UTF8Encoding]::new($false))
        try { $writer.Write($manifestJson) } finally { $writer.Dispose() }
        foreach ($path in $entries.Keys) {
            $null = [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $entries[$path], "files/$path", [IO.Compression.CompressionLevel]::Optimal)
        }
    } finally { $zip.Dispose() }
    [IO.File]::Move($temporary, $destination)
} finally {
    if (Test-Path -LiteralPath $temporary -PathType Leaf) { Remove-Item -LiteralPath $temporary }
}

Write-Host "Created $destination"
if ($GitHubRepository -or $ReleaseAssets) {
    $filename = [IO.Path]::GetFileName($destination)
    $entry = [ordered]@{
        manifest = $manifest
        sha256 = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        size = (Get-Item -LiteralPath $destination).Length
    }
    if ($ReleaseAssets) { $entry.asset = 'mod.zip' }
    else { $entry.url = "https://github.com/$GitHubRepository/releases/download/$Id-v$Version/$filename" }
    $entryJson = $entry | ConvertTo-Json -Depth 10
    if ($CatalogEntryPath) {
        $entryStream = [IO.File]::Open($entryPath, [IO.FileMode]::CreateNew)
        try {
            $writer = [IO.StreamWriter]::new($entryStream, [Text.UTF8Encoding]::new($false))
            try { $writer.WriteLine($entryJson) } finally { $writer.Dispose() }
        } finally { $entryStream.Dispose() }
        Write-Host "Created $entryPath"
    }
    $entryJson
}
