Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        ScanHandprint()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iAudioCooldownID
        bAudioCooldownActive = False
    EndIf
EndEvent

Function ScanHandprint()
    Quest bunkerQuest = GetOwningQuest()
    Actor playerRef = Game.GetPlayer()
    If bunkerQuest == None || playerRef == None
        Return
    EndIf
    If !bunkerQuest.IsStageDone(iInitialStageToSet)
        bunkerQuest.SetStage(iInitialStageToSet)
    EndIf
    If bunkerQuest.IsStageDone(iInitalShutdownStage) || playerRef.GetValue(EN01_PlayerTriggeredReset) >= 1.0
        If !bunkerQuest.IsStageDone(iCredentialsUpdatedStage)
            bunkerQuest.SetStage(iCredentialsUpdatedStage)
        EndIf
        Return
    EndIf
    If !bAudioCooldownActive
        ObjectReference voiceRef = GenericMachineVoice.GetRef()
        If voiceRef != None
            voiceRef.Say(EN01_HandprintAccessDenied, akTarget = playerRef)
        EndIf
        bAudioCooldownActive = True
        StartTimer(iAudioCooldownLength as Float, iAudioCooldownID)
    EndIf
EndFunction
