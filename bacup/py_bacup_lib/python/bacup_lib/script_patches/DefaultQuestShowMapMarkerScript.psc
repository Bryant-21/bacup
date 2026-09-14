Function EnableMapMarkersForStage(Int aiStage)
    If MapMarkerData == None
        Return
    EndIf

    Int index = 0
    While index < MapMarkerData.Length
        If MapMarkerData[index].StageToEnableMapMarker == aiStage && MapMarkerData[index].MapMarkerAlias != None
            ObjectReference mapMarker = MapMarkerData[index].MapMarkerAlias.GetReference()
            If mapMarker != None
                mapMarker.AddToMap(MapMarkerData[index].MarkDiscovered)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Event OnQuestInit()
    If MapMarkerData != None
        Int index = 0
        While index < MapMarkerData.Length
            If IsStageDone(MapMarkerData[index].StageToEnableMapMarker)
                EnableMapMarkersForStage(MapMarkerData[index].StageToEnableMapMarker)
            EndIf
            index += 1
        EndWhile
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    EnableMapMarkersForStage(auiStageID)
EndEvent
