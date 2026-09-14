Event OnQuestInit()
    EWS = (Self as Quest) as DefaultQuestEncounterWaveScript
EndEvent

Function StartGauntletWave(Int aiWaveIndex)
    If EWS == None
        EWS = (Self as Quest) as DefaultQuestEncounterWaveScript
    EndIf
    If EWS != None
        EWS.StartLocalEncounterWave(aiWaveIndex)
    EndIf
EndFunction

Function UnlockLaserGrid(Int aiGridIndex)
    If LaserGrids == None || aiGridIndex < 0 || aiGridIndex >= LaserGrids.GetCount()
        Return
    EndIf

    ObjectReference gridRef = LaserGrids.GetAt(aiGridIndex)
    If gridRef != None
        gridRef.DisableNoWait()
    EndIf
EndFunction

Function UnlockAllLaserGrids()
    If LaserGrids == None
        Return
    EndIf

    Int gridIndex = 0
    While gridIndex < LaserGrids.GetCount()
        UnlockLaserGrid(gridIndex)
        gridIndex += 1
    EndWhile
EndFunction
