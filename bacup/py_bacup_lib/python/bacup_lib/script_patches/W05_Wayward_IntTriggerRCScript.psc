Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If OwningPlayer == None || OwningPlayer.GetActorReference() == None
        Return
    EndIf
    If OwningPlayer.GetActorReference().GetValue(W05_Wayward_PlayerCompletedQuestline) >= 1.0
        Return
    EndIf
    akActionRef.AddKeyword(W05_Wayward_Interior_RandomConvoHandlerStartKeyword)
EndEvent
