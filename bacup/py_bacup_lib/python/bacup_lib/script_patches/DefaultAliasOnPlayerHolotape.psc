Event OnHolotapePlay(ObjectReference akTerminalRef)
    TryAdvanceHolotape()
EndEvent

Function TryAdvanceHolotape()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(HolotapeToWatchFor) > 0
        TryToSetStage(PlayerCheckOverride = True, RefToCheck = playerRef, FormToCheck = HolotapeToWatchFor)
    EndIf
EndFunction
