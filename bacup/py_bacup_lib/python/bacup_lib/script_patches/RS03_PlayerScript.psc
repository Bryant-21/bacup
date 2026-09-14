Function RegisterAutoDocActivation()
    If AutoDoc == None
        Return
    EndIf
    ObjectReference autoDocRef = AutoDoc.GetReference()
    If autoDocRef != None
        UnregisterForRemoteEvent(autoDocRef, "OnActivate")
        RegisterForRemoteEvent(autoDocRef, "OnActivate")
    EndIf
EndFunction

Event OnAliasInit()
    RegisterAutoDocActivation()
EndEvent

Event OnPlayerLoadGame()
    RegisterAutoDocActivation()
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor playerRef = GetActorReference()
    Quest owningQuest = GetOwningQuest()
    If playerRef != None && owningQuest != None && akActionRef == playerRef && owningQuest.IsStageDone(600) && !owningQuest.IsStageDone(1000)
        owningQuest.SetStage(1000)
    EndIf
EndEvent
