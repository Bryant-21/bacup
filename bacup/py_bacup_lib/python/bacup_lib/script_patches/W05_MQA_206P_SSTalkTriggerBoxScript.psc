Event OnTriggerEnter(ObjectReference akActionRef)
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = None

    If Alias_Player != None
        playerRef = Alias_Player.GetActorReference()
    EndIf
    If owningQuest == None || playerRef == None || akActionRef != playerRef
        Return
    EndIf
    If owningQuest.IsStageDone(StageToSet) || owningQuest.IsStageDone(TurnOffStage) || playerRef.IsInCombat()
        Return
    EndIf

    owningQuest.SetStage(StageToSet)
EndEvent
