Function StartHarvesterDefense(Int aiWaveIndex, Int aiActivationStage, Int aiCompletionStage)
    If !IsStageDone(aiActivationStage) || IsStageDone(aiCompletionStage)
        Return
    EndIf

    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveController != None
        waveController.StartLocalEncounterWave(aiWaveIndex)
    EndIf

    CancelTimer(aiCompletionStage)
    StartTimer(45.0, aiCompletionStage)
EndFunction

Function FinishHarvesterDefense(Int aiActivationStage, Int aiCompletionStage)
    CancelTimer(aiCompletionStage)
    If IsStageDone(aiActivationStage) && !IsStageDone(aiCompletionStage)
        SetStage(aiCompletionStage)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 220
        FinishHarvesterDefense(210, 220)
    ElseIf aiTimerID == 320
        FinishHarvesterDefense(310, 320)
    ElseIf aiTimerID == 420
        FinishHarvesterDefense(410, 420)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(220)
    CancelTimer(320)
    CancelTimer(420)
EndEvent
