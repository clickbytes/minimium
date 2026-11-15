function Backup-EarthWorldForRebuild {
    param(
        [Parameter(Mandatory)][string]$WorldDirectory,
        [Parameter(Mandatory)][string]$ProfileDirectory
    )

    $stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss') + '-' +
        [Guid]::NewGuid().ToString('N').Substring(0, 8)
    $entries = @(
        @{ Path = $WorldDirectory; LockName = 'server.lock'; Kind = 'world';
            Markers = @('world.planet.ron', 'world.snapshot', 'world.seed', 'server.lock') },
        @{ Path = $ProfileDirectory; LockName = 'player.profile.lock'; Kind = 'profile';
            Markers = @('player.profile', 'player.profile.lock') }
    )
    foreach ($entry in $entries) {
        $entry.Path = [IO.Path]::GetFullPath($entry.Path).TrimEnd('\', '/')
        $parent = [IO.Path]::GetDirectoryName($entry.Path)
        if (-not $parent -or $entry.Path -eq [IO.Path]::GetPathRoot($entry.Path).TrimEnd('\', '/')) {
            throw 'RebuildWorld requires specific world and profile directories, not filesystem roots.'
        }
        $entry.Backup = "$($entry.Path).backup-$stamp"
        if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($entry.Backup)) -ne $parent) {
            throw 'The rebuild backup escaped its intended parent directory.'
        }
        if (Test-Path -LiteralPath $entry.Backup) {
            throw "The rebuild backup already exists: $($entry.Backup)"
        }
        $entry.Stream = $null
        $entry.Locked = $false
        $entry.Moved = $false
    }
    $world = $entries[0].Path
    $profile = $entries[1].Path
    if ($world -eq $profile -or
        $world.StartsWith($profile + '\', [StringComparison]::OrdinalIgnoreCase) -or
        $profile.StartsWith($world + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'RebuildWorld requires separate, non-nested -DataDir and -ProfileDir directories.'
    }

    # Validate both targets before locking or moving either directory.
    foreach ($entry in $entries) {
        if (-not (Test-Path -LiteralPath $entry.Path)) { continue }
        $item = Get-Item -LiteralPath $entry.Path -Force
        if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "RebuildWorld requires an ordinary directory: $($entry.Path)"
        }
        $recognized = $entry.Markers | Where-Object {
            Test-Path -LiteralPath (Join-Path $entry.Path $_) -PathType Leaf
        }
        if (-not $recognized -and (Get-ChildItem -LiteralPath $entry.Path -Force | Select-Object -First 1)) {
            throw "Refusing to move an unrecognized $($entry.Kind) directory: $($entry.Path)"
        }
    }

    try {
        # Match the server/profile OS locks and hold both throughout the move.
        foreach ($entry in $entries) {
            if (-not (Test-Path -LiteralPath $entry.Path)) { continue }
            try {
                $entry.Stream = [IO.FileStream]::new((Join-Path $entry.Path $entry.LockName),
                    [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite,
                    ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
                $entry.Stream.Lock(0, 1)
                $entry.Locked = $true
            } catch {
                throw "Cannot rebuild while the $($entry.Kind) is in use or inaccessible: $($entry.Path). Close its client/server first. $($_.Exception.Message)"
            }
        }
        foreach ($entry in $entries) {
            if (-not $entry.Locked) { continue }
            Move-Item -LiteralPath $entry.Path -Destination $entry.Backup -ErrorAction Stop
            $entry.Moved = $true
        }
    } catch {
        $failure = $_
        # If only one move succeeded, restore it so the original pair stays usable.
        foreach ($entry in $entries) {
            if (-not $entry.Moved) { continue }
            try {
                if (Test-Path -LiteralPath $entry.Path) {
                    throw "The original path has been recreated: $($entry.Path)"
                }
                Move-Item -LiteralPath $entry.Backup -Destination $entry.Path -ErrorAction Stop
            } catch {
                Write-Warning "Could not restore $($entry.Kind); its data is preserved at $($entry.Backup): $($_.Exception.Message)"
            }
        }
        throw $failure
    } finally {
        foreach ($entry in $entries) {
            if ($entry.Stream) {
                try {
                    if ($entry.Locked) { $entry.Stream.Unlock(0, 1) }
                } finally {
                    $entry.Stream.Dispose()
                }
            }
        }
    }
    foreach ($entry in $entries) {
        if ($entry.Moved) { Write-Host "Previous $($entry.Kind) preserved at $($entry.Backup)" }
    }
}
