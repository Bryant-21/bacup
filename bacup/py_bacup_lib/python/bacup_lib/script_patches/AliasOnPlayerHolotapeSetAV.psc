Event OnHolotapePlay(ObjectReference akTerminalRef)
    If setAVonEnd
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef && HolotapeAV && HolotapeToWatchFor && playerRef.GetItemCount(HolotapeToWatchFor) > 0
        If playerRef.GetValue(HolotapeAV) != Value
            playerRef.SetValue(HolotapeAV, Value as Float)
        EndIf
    EndIf
EndEvent
