Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If currentPlayer != None && currentPlayer.GetReference() != None && akActionRef != currentPlayer.GetReference()
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf

    ; Stage 1300 is reachable from the projector or the Vault 79 plans; 1400 retires it.
    If owningQuest.IsStageDone(1300) || owningQuest.IsStageDone(1400)
        Return
    EndIf

    owningQuest.SetStage(1300)
EndEvent
