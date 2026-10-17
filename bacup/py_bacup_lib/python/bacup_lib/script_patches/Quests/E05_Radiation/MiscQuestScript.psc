; [Misc Quest to track rads and pointer]: while the player carries RadAmount or more rads, objective 10 points at the
; Radiation Rumble decontamination room; once the rads are cleared the pointer completes and the quest stops.
Event OnQuestInit()
    PlayerRef = Game.GetPlayer()
    If Player_Alias != None && Player_Alias.GetReference() == None
        Player_Alias.ForceRefTo(PlayerRef)
    EndIf
    StartTimer(1.0, 1)
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning()
        Return
    EndIf
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
    EndIf
    Float threshold = RadAmount
    If threshold <= 0.0
        threshold = 600.0
    EndIf
    Float currentRads = PlayerRef.GetValue(Rads)
    If currentRads >= threshold
        If !IsObjectiveDisplayed(10)
            SetObjectiveDisplayed(10, True)
        EndIf
    ElseIf IsObjectiveDisplayed(10)
        SetObjectiveCompleted(10, True)
        Stop()
        Return
    Else
        Quest radiationEvent = Game.GetFormFromFile(0x562877, "SeventySix.esm") as Quest
        If radiationEvent == None || !radiationEvent.IsRunning()
            Stop()
            Return
        EndIf
    EndIf
    StartTimer(2.0, 1)
EndEvent
