#Requires -Version 7.0
$ErrorActionPreference = 'Stop'
$dest = Join-Path $PSScriptRoot 'standard-licenses'
New-Item -ItemType Directory -Force $dest | Out-Null
$records = @()
foreach ($id in @('0BSD','Apache-2.0','BSD-2-Clause','BSD-3-Clause','BSL-1.0','CC0-1.0','ISC','MIT','MPL-2.0','NCSA','Unicode-3.0','Unlicense','Zlib','LGPL-2.1-or-later','LLVM-exception')) {
    $url = "https://raw.githubusercontent.com/spdx/license-list-data/main/text/$id.txt"
    Invoke-WebRequest $url -OutFile "$dest/$id.txt"
    $records += @{ license=$id; source=$url; sha256=(Get-FileHash "$dest/$id.txt").Hash }
}
$records | ConvertTo-Json | Set-Content "$dest/provenance.json"
@'
These are supplemental standard license texts from SPDX. Original component
copyright notices, license declarations and source files are preserved in the
component directories and source archives. Standard templates do not replace
the original notices or attribute copyright to a new author.
'@ | Set-Content "$dest/README.txt"
