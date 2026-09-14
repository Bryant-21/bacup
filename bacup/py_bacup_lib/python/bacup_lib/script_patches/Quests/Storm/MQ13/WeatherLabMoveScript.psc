Event OnTriggerEnter(ObjectReference akActionRef)
    Quest owningQuest = GetOwningQuest()
    If akActionRef != Game.GetPlayer() || owningQuest == None
        Return
    EndIf
    If !owningQuest.IsStageDone(iStageToMove) || owningQuest.IsStageDone(iTurnOffStage)
        Return
    EndIf

    ObjectReference destination = AliasToMoveTo.GetReference()
    If destination != None
        akActionRef.MoveTo(destination, 0.0, 0.0, 0.0, bMatchRotation)
    EndIf
EndEvent
