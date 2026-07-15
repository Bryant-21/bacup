Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    Int index = 0
    While index < GetCount()
        Actor bossRef = GetActorAt(index)
        If bossRef != None && !bossRef.IsDead()
            Return
        EndIf
        index += 1
    EndWhile

    Actor playerRef = Game.GetPlayer()
    If POI286_GauleyMineExitKey != None && playerRef.GetItemCount(POI286_GauleyMineExitKey) < 1
        playerRef.AddItem(POI286_GauleyMineExitKey, 1, True)
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && StageToSet > 0 && !owningQuest.IsStageDone(StageToSet)
        owningQuest.SetStage(StageToSet)
    EndIf
EndEvent
