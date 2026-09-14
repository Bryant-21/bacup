Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP00_AV_StartedRSVP01, 1.0)
    EndIf
EndFunction
