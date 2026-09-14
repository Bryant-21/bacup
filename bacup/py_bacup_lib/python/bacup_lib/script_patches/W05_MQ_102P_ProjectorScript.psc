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

    ; Stage 1300 owns the Vault 79 presentation scene; 1400 retires it.
    If owningQuest.IsStageDone(1300) || owningQuest.IsStageDone(1400)
        Return
    EndIf

    owningQuest.SetStage(1300)
EndEvent
