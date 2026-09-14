Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTarget)
    Actor playerRef = Game.GetPlayer()
    If playerRef && AVToSet && playerRef.GetValue(AVToSet) != SetToValue
        playerRef.SetValue(AVToSet, SetToValue as Float)
    EndIf
EndEvent
