Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_RS01B_Contact_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && RS01B_Contact_Started != None && playerRef.GetValue(RS01B_Contact_Started) < 1.0
        playerRef.SetValue(RS01B_Contact_Started, 1.0)
    EndIf
    If MorgantownAirportMarker != None
        MorgantownAirportMarker.AddToMap()
    EndIf
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0125_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
    Actor playerRef = Alias_RS01B_Contact_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && RS01B_CheckpointValue != None && playerRef.GetValue(RS01B_CheckpointValue) < 10.0
        playerRef.SetValue(RS01B_CheckpointValue, 10.0)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True)
    Actor playerRef = Alias_RS01B_Contact_Player.GetActorReference()
    If playerRef != None && RS01B_CheckpointValue != None
        playerRef.SetValue(RS01B_CheckpointValue, 20.0)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(400, True)
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(400, True)
    Actor playerRef = Alias_RS01B_Contact_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None
        Return
    EndIf

    If RS01B_Contact_Completed != None
        playerRef.SetValue(RS01B_Contact_Completed, 1.0)
    EndIf
    If RS03_Inoculation_Started != None && playerRef.GetValue(RS03_Inoculation_Started) < 1.0
        If RS03_Inoculation_Keyword != None
            RS03_Inoculation_Keyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
    EndIf
    If RS06_Manual_Stims_Keyword != None
        RS06_Manual_Stims_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
