Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && HouseOfScaresStatus != None
        playerRef.SetValue(HouseOfScaresStatus, 1.0)
    EndIf
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && HouseOfScaresStatus != None
        playerRef.SetValue(HouseOfScaresStatus, 2.0)
    EndIf
    SetObjectiveCompleted(100, True)
    CompleteQuest()
    Stop()
EndFunction
