Function Fragment_Stage_0100_Item_00()
    ObjectReference pipeRef = Alias_Pipe.GetReference()
    If pipeRef != None
        pipeRef.SetDestroyed(False)
    EndIf
    SetObjectiveDisplayed(100, True, True)

    defaultquestencounterwavescript encounterWaves = (Self as Quest) as defaultquestencounterwavescript
    If encounterWaves != None
        encounterWaves.StartLocalEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    defaultquestencounterwavescript encounterWaves = (Self as Quest) as defaultquestencounterwavescript
    If encounterWaves != None
        encounterWaves.StartLocalEncounterWave(1)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    defaultquestencounterwavescript encounterWaves = (Self as Quest) as defaultquestencounterwavescript
    If encounterWaves != None
        encounterWaves.StartLocalEncounterWave(2)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(100, True)
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    SetObjectiveFailed(100, True)
    Stop()
EndFunction
