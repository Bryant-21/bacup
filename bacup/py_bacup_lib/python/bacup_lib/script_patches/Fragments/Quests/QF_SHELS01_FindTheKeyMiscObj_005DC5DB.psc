Function Fragment_Stage_0300_Item_00()
    Actor playerRef = thePlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(playerKnowsPasswordAV, 1.0)
    EndIf
EndFunction
