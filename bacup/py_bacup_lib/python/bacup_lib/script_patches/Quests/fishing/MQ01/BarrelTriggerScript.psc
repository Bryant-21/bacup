Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = akActionRef as Actor
    If playerRef == None || playerRef != Game.GetPlayer()
        Return
    EndIf

    ObjectReference activateObj = None
    If ActivateLinkedRefKeyword != None
        activateObj = GetLinkedRef(ActivateLinkedRefKeyword)
        If activateObj != None && (ObjectTypeBook == None || activateObj.HasKeyword(ObjectTypeBook))
            activateObj.Activate(playerRef, False)
        EndIf
    EndIf

    If QuestToStart != None && !QuestToStart.IsRunning() && !QuestToStart.IsCompleted()
        QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf

    If QuestToStart != None && QuestToStart.IsRunning()
        If QuestStageOverride >= 0 && !QuestToStart.IsStageDone(QuestStageOverride)
            QuestToStart.SetStage(QuestStageOverride)
        EndIf

        Int markerIndex = 0
        While MapMarkersToAdd != None && markerIndex < MapMarkersToAdd.Length
            If MapMarkersToAdd[markerIndex] != None
                MapMarkersToAdd[markerIndex].AddToMap(bMarkDiscovered)
            EndIf
            markerIndex += 1
        EndWhile

        If MessageToShow != None
            MessageToShow.Show()
        EndIf
    EndIf
EndEvent
