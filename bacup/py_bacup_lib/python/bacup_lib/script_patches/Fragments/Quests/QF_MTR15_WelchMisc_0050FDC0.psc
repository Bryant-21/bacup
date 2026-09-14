Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(MTR15_MiscComplete, 1.0)
    EndIf
EndFunction
