Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && WS02status != None
        playerRef.SetValue(WS02status, 1.0)
    EndIf
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    MarkCollected(WS02Holotape01)
EndFunction

Function Fragment_Stage_0300_Item_00()
    MarkCollected(WS02Holotape02)
EndFunction

Function Fragment_Stage_0400_Item_00()
    MarkCollected(WS02Holotape03)
EndFunction

Function Fragment_Stage_0500_Item_00()
    MarkCollected(WS02Holotape04)
EndFunction

Function Fragment_Stage_0600_Item_00()
    MarkCollected(WS02Holotape05)
EndFunction

Function Fragment_Stage_0700_Item_00()
    MarkCollected(WS02Holotape06)
EndFunction

Function Fragment_Stage_0800_Item_00()
    MarkCollected(WS02Holotape07)
EndFunction

Function MarkCollected(ActorValue collectedValue)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None || collectedValue == None
        Return
    EndIf
    playerRef.SetValue(collectedValue, 1.0)
    If playerRef.GetValue(WS02Holotape01) >= 1.0 && playerRef.GetValue(WS02Holotape02) >= 1.0 && playerRef.GetValue(WS02Holotape03) >= 1.0 && playerRef.GetValue(WS02Holotape04) >= 1.0 && playerRef.GetValue(WS02Holotape05) >= 1.0 && playerRef.GetValue(WS02Holotape06) >= 1.0 && playerRef.GetValue(WS02Holotape07) >= 1.0 && !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && WS02status != None
        playerRef.SetValue(WS02status, 2.0)
    EndIf
    SetObjectiveCompleted(100, True)
    CompleteQuest()
    Stop()
EndFunction
