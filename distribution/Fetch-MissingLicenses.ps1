#Requires -Version 7.0
$ErrorActionPreference = 'Stop'
$cache = @{}
foreach ($component in @('glazebar','glazewm')) {
    foreach ($crate in Get-ChildItem (Join-Path $PSScriptRoot "vendor-$component") -Directory) {
        if (Get-ChildItem $crate.FullName -Recurse -File | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|NOTICE|AUTHORS|COPYRIGHT)') { continue }
        $dest = Join-Path $PSScriptRoot "upstream-notices/$component/$($crate.Name)"
        if (Test-Path (Join-Path $dest 'provenance.json')) { continue }
        $toml = Get-Content (Join-Path $crate.FullName 'Cargo.toml') -Raw
        $repository = [regex]::Match($toml, '(?m)^repository\s*=\s*"([^"]+)"').Groups[1].Value.TrimEnd('/').Replace('.git','')
        $vcsFile = Join-Path $crate.FullName '.cargo_vcs_info.json'
        $vcs = if (Test-Path $vcsFile) { Get-Content $vcsFile -Raw | ConvertFrom-Json } else { $null }
        if ($repository -notmatch '^https://github.com/([^/]+/[^/]+)$' -or !$vcs.git.sha1) {
            New-Item -ItemType Directory -Force $dest | Out-Null
            Copy-Item (Join-Path $crate.FullName 'Cargo.toml') (Join-Path $dest 'LICENSE-DECLARATION.toml')
            @{ declarationOnly=$true; repository=$repository; reason='No pinned VCS metadata. Original license declaration preserved; standard texts are in licenses/standard.' } | ConvertTo-Json | Set-Content (Join-Path $dest 'provenance.json')
            continue
        }
        $github = $Matches[1]
        $key = "$github/$($vcs.git.sha1)"
        if (!$cache.ContainsKey($key)) { $cache[$key] = Invoke-RestMethod "https://api.github.com/repos/$github/git/trees/$($vcs.git.sha1)?recursive=1" }
        $documents = @($cache[$key].tree | Where-Object { $_.type -eq 'blob' -and (Split-Path $_.path -Leaf) -match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)([._-]|$)' })
        $cratePath = [string]$vcs.path_in_vcs
        $documents = @($documents | Where-Object { $parent = Split-Path $_.path -Parent; !$parent -or $cratePath.Replace('/','\').StartsWith($parent.Replace('/','\'), [StringComparison]::Ordinal) })
        if (!$documents.Count) {
            New-Item -ItemType Directory -Force $dest | Out-Null
            Copy-Item (Join-Path $crate.FullName 'Cargo.toml') (Join-Path $dest 'LICENSE-DECLARATION.toml')
            @{ declarationOnly=$true; repository=$repository; revision=$vcs.git.sha1; reason='Upstream has no standalone license file. Original license declaration and authors preserved; standard terms are in licenses/standard.' } | ConvertTo-Json | Set-Content (Join-Path $dest 'provenance.json')
            continue
        }
        New-Item -ItemType Directory -Force $dest | Out-Null
        $records = @()
        foreach ($document in $documents) {
            $url = "https://raw.githubusercontent.com/$github/$($vcs.git.sha1)/$($document.path)"
            $name = $document.path.Replace('/','__')
            Invoke-WebRequest $url -OutFile (Join-Path $dest $name)
            $records += [ordered]@{ url=$url; file=$name; sha256=(Get-FileHash (Join-Path $dest $name)).Hash }
        }
        $records | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $dest 'provenance.json')
        Write-Output "Recovered upstream notices: $component/$($crate.Name)"
    }
}
