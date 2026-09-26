Event OnRead()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If KeyToGive != None && playerRef.GetItemCount(KeyToGive) == 0
        playerRef.AddItem(KeyToGive, 1, True)
    EndIf
    If ValueToSet != None
        playerRef.SetValue(ValueToSet, 1.0)
    EndIf
EndEvent
