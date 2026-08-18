param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot '..\src-tauri\target\release\dsh-desktop.exe'),
    [ValidateRange(10, 86400)][int]$DurationSeconds = 60,
    [ValidateRange(0, 300)][int]$WarmupSeconds = 15,
    [ValidateRange(250, 5000)][int]$SampleIntervalMs = 1000
)

$ErrorActionPreference = 'Stop'
$resolvedExecutable = [IO.Path]::GetFullPath($ExecutablePath)
if (-not (Test-Path -LiteralPath $resolvedExecutable -PathType Leaf)) {
    throw "找不到待测程序：$resolvedExecutable"
}

function Get-ProcessTreeIds {
    param([uint32]$RootProcessId)

    $allProcesses = Get-CimInstance Win32_Process
    $ids = [System.Collections.Generic.HashSet[uint32]]::new()
    [void]$ids.Add($RootProcessId)
    do {
        $previousCount = $ids.Count
        foreach ($item in $allProcesses) {
            if ($ids.Contains([uint32]$item.ParentProcessId)) {
                [void]$ids.Add([uint32]$item.ProcessId)
            }
        }
    } while ($ids.Count -gt $previousCount)

    return @($ids | ForEach-Object { $_ })
}

function Get-TreeSnapshot {
    param([uint32[]]$ProcessIds)

    $processes = @(Get-Process -Id $ProcessIds -ErrorAction SilentlyContinue)
    $workingSetBytes = ($processes | Measure-Object WorkingSet64 -Sum).Sum
    $privateBytes = ($processes | Measure-Object PrivateMemorySize64 -Sum).Sum
    $privateWorkingSetBytes = (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process |
        Where-Object { [uint32]$_.IDProcess -in $ProcessIds } |
        Measure-Object WorkingSetPrivate -Sum).Sum
    $cpuSeconds = ($processes | Measure-Object CPU -Sum).Sum
    return [pscustomobject]@{
        Count = $processes.Count
        WorkingSetMiB = [double]$workingSetBytes / 1MB
        PrivateBytesMiB = [double]$privateBytes / 1MB
        PrivateWorkingSetMiB = [double]$privateWorkingSetBytes / 1MB
        CpuMs = [double]$cpuSeconds * 1000
    }
}

function Get-Percentile {
    param([double[]]$Values, [double]$Percentile)

    if ($Values.Count -eq 0) { return 0 }
    $sorted = @($Values | Sort-Object)
    $index = [Math]::Max(0, [Math]::Ceiling($sorted.Count * $Percentile) - 1)
    return $sorted[$index]
}

$rootProcess = $null
$startupWatch = [Diagnostics.Stopwatch]::StartNew()
try {
    $rootProcess = Start-Process -FilePath $resolvedExecutable -PassThru -WindowStyle Hidden
    $ready = $false
    while (-not $ready -and $startupWatch.Elapsed.TotalSeconds -lt 30) {
        if ($rootProcess.HasExited) { throw '应用在就绪前退出' }
        $treeIds = Get-ProcessTreeIds -RootProcessId $rootProcess.Id
        $listeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
            Where-Object { $_.LocalAddress -eq '127.0.0.1' -and $_.OwningProcess -in $treeIds })
        foreach ($listener in $listeners) {
            try {
                $response = Invoke-WebRequest -UseBasicParsing `
                    -Uri "http://127.0.0.1:$($listener.LocalPort)/" -TimeoutSec 2
                $ready = $response.StatusCode -eq 200
            } catch { $ready = $false }
            if ($ready) { break }
        }
        if (-not $ready) { Start-Sleep -Milliseconds 100 }
    }
    if (-not $ready) { throw '应用在 30 秒内未就绪' }
    $startupWatch.Stop()
    if ($WarmupSeconds -gt 0) { Start-Sleep -Seconds $WarmupSeconds }

    $logicalProcessors = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
    $workingSetSamples = [System.Collections.Generic.List[double]]::new()
    $privateBytesSamples = [System.Collections.Generic.List[double]]::new()
    $privateWorkingSetSamples = [System.Collections.Generic.List[double]]::new()
    $cpuSamples = [System.Collections.Generic.List[double]]::new()
    $previousIds = Get-ProcessTreeIds -RootProcessId $rootProcess.Id
    $previousSnapshot = Get-TreeSnapshot -ProcessIds $previousIds
    $previousTime = [Diagnostics.Stopwatch]::StartNew()
    $sampleWatch = [Diagnostics.Stopwatch]::StartNew()

    while ($sampleWatch.Elapsed.TotalSeconds -lt $DurationSeconds) {
        Start-Sleep -Milliseconds $SampleIntervalMs
        $treeIds = Get-ProcessTreeIds -RootProcessId $rootProcess.Id
        $snapshot = Get-TreeSnapshot -ProcessIds $treeIds
        $elapsedMs = $previousTime.Elapsed.TotalMilliseconds
        $cpuPercent = if ($elapsedMs -gt 0) {
            ($snapshot.CpuMs - $previousSnapshot.CpuMs) / $elapsedMs / $logicalProcessors * 100
        } else { 0 }
        $workingSetSamples.Add($snapshot.WorkingSetMiB)
        $privateBytesSamples.Add($snapshot.PrivateBytesMiB)
        $privateWorkingSetSamples.Add($snapshot.PrivateWorkingSetMiB)
        $cpuSamples.Add([Math]::Max(0, $cpuPercent))
        $previousSnapshot = $snapshot
        $previousTime.Restart()
    }

    $capturedIds = Get-ProcessTreeIds -RootProcessId $rootProcess.Id
    $cleanupWatch = [Diagnostics.Stopwatch]::StartNew()
    # 默认关闭行为会隐藏到托盘；资源脚本强制结束根进程，仅验证 Job Object 回收能力。
    Stop-Process -Id $rootProcess.Id -Force
    do {
        $residual = @(Get-Process -Id $capturedIds -ErrorAction SilentlyContinue)
        if ($residual.Count -eq 0 -or $cleanupWatch.Elapsed.TotalSeconds -ge 3) { break }
        Start-Sleep -Milliseconds 100
    } while ($true)
    $cleanupWatch.Stop()

    $firstMemory = if ($privateWorkingSetSamples.Count) { $privateWorkingSetSamples[0] } else { 0 }
    $lastMemory = if ($privateWorkingSetSamples.Count) {
        $privateWorkingSetSamples[$privateWorkingSetSamples.Count - 1]
    } else { 0 }
    [ordered]@{
        executable = $resolvedExecutable
        durationSeconds = $DurationSeconds
        warmupSeconds = $WarmupSeconds
        coldStartMs = [Math]::Round($startupWatch.Elapsed.TotalMilliseconds)
        workingSetPeakMiB = [Math]::Round(($workingSetSamples | Measure-Object -Maximum).Maximum, 2)
        privateBytesPeakMiB = [Math]::Round(($privateBytesSamples | Measure-Object -Maximum).Maximum, 2)
        privateWorkingSetPeakMiB = [Math]::Round(
            ($privateWorkingSetSamples | Measure-Object -Maximum).Maximum,
            2
        )
        privateWorkingSetGrowthMiB = [Math]::Round($lastMemory - $firstMemory, 2)
        cpuP95Percent = [Math]::Round((Get-Percentile -Values $cpuSamples -Percentile 0.95), 3)
        cleanupMs = [Math]::Round($cleanupWatch.Elapsed.TotalMilliseconds)
        cleanupMode = 'forced-root-process'
        residualProcesses = $residual.Count
    } | ConvertTo-Json
} finally {
    if ($null -ne $rootProcess -and -not $rootProcess.HasExited) {
        Stop-Process -Id $rootProcess.Id -Force -ErrorAction SilentlyContinue
    }
}
