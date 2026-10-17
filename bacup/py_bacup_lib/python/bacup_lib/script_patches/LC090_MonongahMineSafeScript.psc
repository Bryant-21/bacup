; LC090 Monongah Mine supplies: the player opening the unlocked safe finds the supplies and completes the misc (100).
Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    ObjectReference safe = GetReference()
    Quest owner = GetOwningQuest()
    If akActionRef != player || safe == None || safe.IsLocked() || owner.IsStageDone(100)
        Return
    EndIf
    If currentPlayer != None
        currentPlayer.ForceRefIfEmpty(player)
    EndIf
    owner.SetStage(100)
EndEvent
