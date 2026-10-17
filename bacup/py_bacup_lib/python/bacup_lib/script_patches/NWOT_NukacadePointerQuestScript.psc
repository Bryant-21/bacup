; Nuka-Cade pointer ("Play <game> at the Nuka-Cade"): objective 0 marks the nearest machine of GameAlias, and playing
; any of them sets CompletedStage and closes the pointer.
Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If Alias_Player != None
        Alias_Player.ForceRefIfEmpty(player)
    EndIf
    ObjectReference nearest = None
    Float nearestDistance = 0.0
    Int index = 0
    While GameAlias != None && index < GameAlias.GetCount()
        ObjectReference machine = GameAlias.GetAt(index)
        If machine != None
            RegisterForRemoteEvent(machine, "OnActivate")
            Float distance = player.GetDistance(machine)
            If nearest == None || distance < nearestDistance
                nearest = machine
                nearestDistance = distance
            EndIf
        EndIf
        index += 1
    EndWhile
    If ObjectiveAlias != None && nearest != None
        ObjectiveAlias.ForceRefTo(nearest)
    EndIf
    SetObjectiveDisplayed(0, True)
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || IsStageDone(CompletedStage)
        Return
    EndIf
    SetObjectiveCompleted(0, True)
    SetStage(CompletedStage)
    CompleteQuest()
    Stop()
EndEvent
