Event OnQuestInit()
    If !GetStageDone(iStartUpStage)
        SetStage(iStartUpStage)
    EndIf
EndEvent

Function OpenFissure()
    If Fissure != None
        SetRefDisabled(Fissure.GetReference(), True)
    EndIf
    SetCoverFXDisabled(True)
    SetSiteMarkerEnabled(True)
EndFunction

Function CloseFissure()
    SetSiteMarkerEnabled(False)
    SetCoverFXDisabled(False)
    If Fissure != None
        SetRefDisabled(Fissure.GetReference(), False)
    EndIf
EndFunction

Function StartFissureWaves()
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StartLocalEncounterWave(iScorchedWaveIndex)
        waves.StartLocalEncounterWave(iScorchbeastWaveIndex)
    EndIf
EndFunction

Function SetCoverFXDisabled(Bool abDisabled)
    If FissureFX == None
        Return
    EndIf
    Int index = 0
    While index < FissureFX.GetCount()
        SetRefDisabled(FissureFX.GetAt(index), abDisabled)
        index += 1
    EndWhile
EndFunction

Function SetSiteMarkerEnabled(Bool abEnabled)
    ; QUST 2D0F66 binds EnableMarker at alias 9, outside this script's properties.
    ReferenceAlias marker = GetAlias(9) as ReferenceAlias
    If marker != None
        SetRefDisabled(marker.GetReference(), !abEnabled)
    EndIf
EndFunction

Function SetRefDisabled(ObjectReference akRef, Bool abDisabled)
    If akRef == None || akRef.IsDisabled() == abDisabled
        Return
    EndIf
    If abDisabled
        akRef.Disable(False)
    Else
        akRef.Enable(False)
    EndIf
EndFunction
