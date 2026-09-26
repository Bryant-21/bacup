Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_Relationship != None && playerRef.GetValue(AV_Relationship) < 1.0
        playerRef.SetValue(AV_Relationship, 1.0)
    EndIf
    SetStage(1000)
EndFunction

Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_Relationship != None && playerRef.GetValue(AV_Relationship) < 2.0
        playerRef.SetValue(AV_Relationship, 2.0)
    EndIf
    SetStage(2000)
EndFunction

Function Fragment_Stage_1000_Item_00()
    If MiscQuest == None || MiscQuest.IsCompleted()
        Return
    EndIf
    If !MiscQuest.IsRunning()
        ; Event-scoped quests must start through their Story Manager route in FO4.
        Keyword startEvent = Game.GetFormFromFile(0x005D1EE0, "SeventySix.esm") as Keyword
        If startEvent != None
            Actor playerRef = Game.GetPlayer()
            startEvent.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef)
        EndIf
    EndIf
    If MiscQuest.IsRunning() && MiscQuest.GetStage() < 400
        MiscQuest.SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    Fragment_Stage_1000_Item_00()
    If MiscQuest != None && MiscQuest.IsRunning() && !MiscQuest.IsCompleted() && MiscQuest.GetStage() < 450
        MiscQuest.SetStage(450)
    EndIf
EndFunction

Function Fragment_Stage_3000_Item_00()
    If MiscQuest != None && MiscQuest.IsRunning() && MiscQuest.IsStageDone(500) && !MiscQuest.IsStageDone(600) && !MiscQuest.IsCompleted()
        MiscQuest.SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_Relationship != None
        playerRef.SetValue(AV_Relationship, 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_Relationship != None
        playerRef.SetValue(AV_Relationship, 4.0)
    EndIf
EndFunction
