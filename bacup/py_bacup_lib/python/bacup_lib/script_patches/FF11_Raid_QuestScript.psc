Event OnQuestInit()
    ; FO76's startup stage runs from here, not from a RunOnStart flag. The retry
    ; covers a SetStage the engine drops while the quest is still starting.
    If !IsStageDone(10)
        SetStage(10)
    EndIf
    StartTimer(1.0, 1168)
EndEvent

Function StartCargoLootWindow()
    Float lootSeconds = 300.0
    If FF11_Raid_CargoLootTimer != None && FF11_Raid_CargoLootTimer.GetValue() > 0.0
        lootSeconds = FF11_Raid_CargoLootTimer.GetValue()
    EndIf
    CancelTimer(1166)
    StartTimer(lootSeconds, 1166)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1166 && IsRunning() && !IsStageDone(9992)
        SetStage(9992)
    ElseIf aiTimerID == 1168 && IsRunning() && !IsStageDone(10)
        SetStage(10)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1166)
    CancelTimer(1168)
EndEvent
