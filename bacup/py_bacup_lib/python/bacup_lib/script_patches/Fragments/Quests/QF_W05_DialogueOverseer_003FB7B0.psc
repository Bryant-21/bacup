Function Fragment_Stage_0500_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && AV_TalkDone != None
        playerRef.SetValue(AV_TalkDone, 1.0)
    EndIf
EndFunction
