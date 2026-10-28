param (
    [string]$HostAddress = "127.0.0.1",
    [int]$Port = 8081,
    [int]$Requests = 300,
    [int]$Concurrent = 30
)

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Starting Stress Benchmark on http://${HostAddress}:${Port}" -ForegroundColor Cyan
Write-Host " Total Requests: $Requests, Concurrency: $Concurrent" -ForegroundColor Cyan
Write-Host " Target Availability: >= 99.5%" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$url = "http://${HostAddress}:${Port}/"

# Use native RunspacePool for in-process high-speed multi-threading
$pool = [System.Management.Automation.Runspaces.RunspaceFactory]::CreateRunspacePool(1, $Concurrent)
$pool.Open()

$scriptBlock = {
    param($targetUrl)
    try {
        $request = [System.Net.HttpWebRequest]::Create($targetUrl)
        $request.Timeout = 3000
        $request.KeepAlive = $true
        $response = [System.Net.HttpWebResponse]$request.GetResponse()
        $code = [int]$response.StatusCode
        $response.Close()
        return $code
    } catch [System.Net.WebException] {
        if ($_.Response) {
            $code = [int]$_.Response.StatusCode
            $_.Response.Close()
            return $code
        }
        return 0
    } catch {
        return 0
    }
}

$jobs = [System.Collections.Generic.List[PSObject]]::new()
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()

for ($i = 0; $i -lt $Requests; $i++) {
    $ps = [System.Management.Automation.PowerShell]::Create()
    $ps.RunspacePool = $pool
    [void]$ps.AddScript($scriptBlock).AddArgument($url)
    $asyncResult = $ps.BeginInvoke()
    $jobs.Add([PSCustomObject]@{
        PowerShell  = $ps
        AsyncResult = $asyncResult
    })
}

$successCount = 0
$failCount = 0

foreach ($job in $jobs) {
    $res = $job.PowerShell.EndInvoke($job.AsyncResult)
    $job.PowerShell.Dispose()
    if ($res.Count -gt 0 -and $res[0] -eq 200) {
        $successCount++
    } else {
        $failCount++
    }
}

$stopwatch.Stop()
$pool.Close()
$pool.Dispose()

$elapsedSeconds = $stopwatch.Elapsed.TotalSeconds
$availability = if ($Requests -gt 0) { ($successCount / $Requests) * 100 } else { 0 }
$rps = if ($elapsedSeconds -gt 0) { [Math]::Round($Requests / $elapsedSeconds, 1) } else { 0 }

Write-Host ""
Write-Host "Benchmark Results:" -ForegroundColor Green
Write-Host "  Completed Requests: $Requests ($successCount succeeded, $failCount failed)"
Write-Host "  Elapsed Time:       $([Math]::Round($elapsedSeconds, 2)) seconds"
Write-Host "  Throughput:         $rps req/sec"
Write-Host "  Availability:       $([Math]::Round($availability, 2))%" -ForegroundColor $(if ($availability -ge 99.5) { "Green" } else { "Yellow" })


