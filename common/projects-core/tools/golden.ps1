# Builds golden schedules on a development core (needs a helper defining Api-Login, Api and Api-Raw against it,
# e.g. the Projects agent's tools/api.ps1): a project exercising every link type, lag, constraint, ALAP, deadline,
# nested summaries and a link from a summary, then the server's scheduler. Writes, per case, the project as read
# BEFORE the schedule and the scheduler's answer (tests/fixtures/*.json).
param([string]$Out = 'E:\kubuno-build\agent-office-projects\wt\common\projects-core\tests\fixtures')
. C:\kubuno-build\agent-office-projects\tools\api.ps1
Api-Login | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null

function New-Case([string]$Name, [scriptblock]$Calendar) {
    $p = (Api POST /api/v1/office/projects @{ title = "Golden $Name"; start_date = '2026-10-05'; end_date = '2026-11-20' }).project
    $id = $p.id
    $P = "/api/v1/office/projects/$id"
    $t = @{}
    function T([string]$key, $body) { $r = (Api POST "$P/tasks" $body).task; $t[$key] = $r.id; $r }
    function U([string]$key, $patch) { Api PATCH "$P/tasks/$($t[$key])" $patch | Out-Null }
    function L([string]$a, [string]$b, [string]$ty = 'FS', [int]$lag = 0) { Api POST "$P/dependencies" @{ from_task_id = $t[$a]; to_task_id = $t[$b]; dep_type = $ty; lag_days = $lag } | Out-Null }

    T 'phase' @{ name = 'Phase A'; task_type = 'summary' } | Out-Null
    T 'a1' @{ name = 'A1'; parent_id = $t['phase']; duration_days = 3 } | Out-Null
    T 'a2' @{ name = 'A2'; parent_id = $t['phase']; duration_days = 2 } | Out-Null
    T 'sub' @{ name = 'Sub-phase'; parent_id = $t['phase']; task_type = 'summary' } | Out-Null
    T 'a3' @{ name = 'A3'; parent_id = $t['sub']; duration_days = 4 } | Out-Null
    T 'b' @{ name = 'B'; duration_days = 5 } | Out-Null
    T 'c' @{ name = 'C'; duration_days = 4 } | Out-Null
    T 'd' @{ name = 'D'; duration_days = 2 } | Out-Null
    T 'm' @{ name = 'Jalon'; task_type = 'milestone'; duration_days = 0 } | Out-Null
    T 'e' @{ name = 'E SNET'; duration_days = 3 } | Out-Null
    T 'f' @{ name = 'F ALAP'; duration_days = 2 } | Out-Null
    T 'g' @{ name = 'G MSO'; duration_days = 1 } | Out-Null
    T 'h' @{ name = 'H FNLT'; duration_days = 2 } | Out-Null
    T 'i' @{ name = 'I deadline'; duration_days = 3 } | Out-Null
    T 'j' @{ name = 'J MFO'; duration_days = 2 } | Out-Null
    T 'k' @{ name = 'K SNLT'; duration_days = 2 } | Out-Null
    T 'l' @{ name = 'L FNET'; duration_days = 3 } | Out-Null
    T 'n' @{ name = 'N after phase'; duration_days = 2 } | Out-Null

    L 'a1' 'a2'
    L 'a2' 'a3' 'FS' 1
    L 'a1' 'b' 'SS' 2
    L 'b' 'c' 'FF' 1
    L 'c' 'd' 'SF'
    L 'a2' 'm' 'FS' -1
    L 'm' 'f'
    L 'd' 'h'
    L 'b' 'i' 'FS' 0
    L 'phase' 'n'
    L 'g' 'j' 'SS' 0

    U 'e' @{ constraint_type = 'SNET'; constraint_date = '2026-10-15' }
    U 'f' @{ constraint_type = 'ALAP' }
    U 'g' @{ constraint_type = 'MSO'; constraint_date = '2026-10-10' }
    U 'h' @{ constraint_type = 'FNLT'; constraint_date = '2026-10-14' }
    U 'i' @{ deadline_date = '2026-10-12' }
    U 'j' @{ constraint_type = 'MFO'; constraint_date = '2026-10-23' }
    U 'k' @{ constraint_type = 'SNLT'; constraint_date = '2026-10-06' }
    U 'l' @{ constraint_type = 'FNET'; constraint_date = '2026-10-30' }

    if ($Calendar) { & $Calendar $P }

    $before = Api-Raw $P
    $cal = Api-Raw "$P/calendars"
    $h =@{ Authorization = "Bearer $KBTOKEN" }
    $after = (Invoke-WebRequest -UseBasicParsing -Method Post -Uri "$KB$P/cpm" -Headers $h -ContentType 'application/json' -Body '{}').Content
    $doc = "{`"case`":`"$Name`",`"before`":$before,`"calendars`":$cal,`"scheduled`":$after}"
    [IO.File]::WriteAllText("$Out\golden-$Name.json", $doc, (New-Object Text.UTF8Encoding $false))
    "$Name -> $id"
}

New-Case 'default-week' $null
New-Case 'six-days-holidays' {
    param($P)
    $c = (Api POST "$P/calendars" @{ name = 'Six jours'; mon = $true; tue = $true; wed = $true; thu = $true; fri = $true; sat = $true; sun = $false; set_as_project_default = $true }).calendar
    Api PUT "$P/calendars/$($c.id)/exceptions" @{ day = '2026-10-08'; is_working = $false; note = 'Fermeture' } | Out-Null
    Api PUT "$P/calendars/$($c.id)/exceptions" @{ day = '2026-10-11'; is_working = $true; note = 'Dimanche travaillé' } | Out-Null
}
