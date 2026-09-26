Float Function WaveEscalationDelay()
    Float delay = 60.0
    If FF01_DeathBlossoms_BreakTimer != None && FF01_DeathBlossoms_BreakTimer.GetValue() > 0.0
        delay = FF01_DeathBlossoms_BreakTimer.GetValue()
    EndIf
    If delay < 1.0
        delay = 1.0
    EndIf
    Return delay
EndFunction

Function BeginWaveEscalation()
    CancelTimer(36191)
    StartTimer(WaveEscalationDelay(), 36191)
EndFunction

Function StopWaveEscalation()
    CancelTimer(36191)
EndFunction

Bool Function EventStillContested()
    Return IsRunning() && !IsStageDone(1000) && !IsStageDone(1500) && !IsStageDone(1510)
EndFunction

Int Function NextWaveStage()
    If !IsStageDone(200)
        Return 200
    ElseIf !IsStageDone(300)
        Return 300
    ElseIf !IsStageDone(500)
        Return 500
    EndIf
    Return -1
EndFunction

Function SetPlantsRemaining(Int aiCount)
    If FF01_DeathBlossoms_PlantsRemaining != None
        FF01_DeathBlossoms_PlantsRemaining.SetValue(aiCount as Float)
    EndIf
EndFunction

Int Function GetPlantsRemaining()
    If FF01_DeathBlossoms_PlantsRemaining == None
        Return 0
    EndIf
    Return FF01_DeathBlossoms_PlantsRemaining.GetValue() as Int
EndFunction

Function ShowFlowerDestroyedMessage()
    If FF01_DeathBlossoms_FlowerDestroyedMessage == None
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If players != None && players.Find(playerRef) < 0
        Return
    EndIf
    FF01_DeathBlossoms_FlowerDestroyedMessage.Show()
EndFunction

Function FlowerDestroyed(ObjectReference akFlowerRef)
    If !EventStillContested()
        Return
    EndIf
    Int remaining = GetPlantsRemaining() - 1
    If remaining < 0
        remaining = 0
    EndIf
    SetPlantsRemaining(remaining)
    ShowFlowerDestroyedMessage()
    If remaining <= 0
        StopWaveEscalation()
        SetStage(1500)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 36191
        Return
    EndIf
    If !EventStillContested()
        Return
    EndIf
    Int nextStage = NextWaveStage()
    If nextStage < 0
        Return
    EndIf
    SetStage(nextStage)
    If NextWaveStage() >= 0
        BeginWaveEscalation()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(36191)
EndEvent
