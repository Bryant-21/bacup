Event OnInit()
    BeginFireCheck()
EndEvent

Event OnCellAttach()
    BeginFireCheck()
EndEvent

Event OnCellDetach()
    CancelTimer(iTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    if aiTimerID == iTimerID
        if HasKeyword(EN06_ActiveFireworksLauncherKeyword)
            FireVolley()
        endif

        StartTimer(iFireCheckTimerLength as Float, iTimerID)
    endif
EndEvent

Function BeginFireCheck()
    CancelTimer(iTimerID)
    StartTimer(iFireCheckTimerLength as Float, iTimerID)
EndFunction

Function FireVolley()
    FormList fireworks
    int shotCount
    if Utility.RandomFloat() > fSingleShotPercentage
        fireworks = EN06_SingleuseFireworks
        shotCount = 1
    else
        fireworks = EN06_MultiuseFireworks
        shotCount = Utility.RandomInt(iMinShots, iMaxNumShots)
    endif

    if fireworks == None
        return
    endif

    int fireworksCount = fireworks.GetSize()
    if fireworksCount == 0
        return
    endif

    int shotIndex = 0

    while shotIndex < shotCount
        Weapon firework = fireworks.GetAt(Utility.RandomInt(0, fireworksCount - 1)) as Weapon
        if firework != None
            firework.Fire(self)
        endif

        shotIndex += 1
    endwhile
EndFunction
