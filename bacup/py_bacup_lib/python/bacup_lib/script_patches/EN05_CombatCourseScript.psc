; Timer IDs 22/23 are local to this script and deliberately clear of the
; countdown timer the parent course script owns.
Function EN05CBT_StartWaveCooldown(Int aiNextWaveStage)
    If iCooldownTimerLength <= 0
        If !IsStageDone(aiNextWaveStage)
            SetStage(aiNextWaveStage)
        EndIf
        Return
    EndIf
    If aiNextWaveStage == iWave03StartStage
        StartTimer(iCooldownTimerLength as Float, 23)
    Else
        StartTimer(iCooldownTimerLength as Float, 22)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 22
        If !IsStageDone(iWave02StartStage)
            SetStage(iWave02StartStage)
        EndIf
    ElseIf aiTimerID == 23
        If !IsStageDone(iWave03StartStage)
            SetStage(iWave03StartStage)
        EndIf
    Else
        Parent.OnTimer(aiTimerID)
    EndIf
EndEvent

Function EN05CBT_MarkCompleted()
    If EN05_CBT_CompletedValue == None
        Return
    EndIf
    If CurrentPlayers != None && CurrentPlayers.GetCount() > 0
        CurrentPlayers.SetValue(EN05_CBT_CompletedValue, 1.0)
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(EN05_CBT_CompletedValue, 1.0)
    EndIf
EndFunction

Function EN05CBT_AnnounceComplete()
    If EN05_CBT_Complete == None
        Return
    EndIf
    ObjectReference speaker = None
    If MasterSgt != None
        speaker = MasterSgt.GetRef()
    EndIf
    If speaker == None && Loudspeaker != None
        speaker = Loudspeaker.GetRef()
    EndIf
    If speaker != None
        speaker.Say(EN05_CBT_Complete)
    EndIf
EndFunction
