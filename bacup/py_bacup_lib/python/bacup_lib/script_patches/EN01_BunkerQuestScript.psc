Event OnQuestInit()
    SetCollectionEnabled(CircuitBreakerHoldingColl, True)
    SetCollectionEnabled(AirFlueHoldingColl, True)
    SetCollectionEnabled(ConduitHoldingColl, True)
    SetCollectionEnabled(Sparks, True)
    SetEnableMarkers(True)
    If QSTEN01PowerUp_4000 != None && IntBunkerSoundSource != None
        QSTEN01PowerUp_4000.Play(IntBunkerSoundSource)
    EndIf
EndEvent

Event OnQuestShutdown()
    SetCollectionEnabled(Sparks, False)
    SetEnableMarkers(False)
    If QSTEN01PowerDown_4000 != None && ExtBunkerSoundSource != None
        QSTEN01PowerDown_4000.Play(ExtBunkerSoundSource)
    EndIf
EndEvent

Function BeginReset()
    If bResetActive
        Return
    EndIf
    bResetActive = True
    SetCollectionEnabled(Sparks, False)
    If QSTEN01PowerDown_4000 != None && IntBunkerSoundSource != None
        QSTEN01PowerDown_4000.Play(IntBunkerSoundSource)
    EndIf
EndFunction

Function FinishReset()
    bResetActive = False
    SetEnableMarkers(True)
    If QSTEN01PowerUp_4000 != None && IntBunkerSoundSource != None
        QSTEN01PowerUp_4000.Play(IntBunkerSoundSource)
    EndIf
EndFunction

Function SetCollectionEnabled(RefCollectionAlias collectionToChange, Bool abEnable)
    If collectionToChange == None
        Return
    EndIf
    Int index = 0
    While index < collectionToChange.GetCount()
        ObjectReference targetRef = collectionToChange.GetAt(index)
        If targetRef != None
            If abEnable
                targetRef.EnableNoWait()
            Else
                targetRef.DisableNoWait()
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetEnableMarkers(Bool abEnable)
    Int index = 0
    While EnableMarkerAliases != None && index < EnableMarkerAliases.Length
        If EnableMarkerAliases[index] != None
            If abEnable
                EnableMarkerAliases[index].TryToEnableNoWait()
            Else
                EnableMarkerAliases[index].TryToDisableNoWait()
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction
