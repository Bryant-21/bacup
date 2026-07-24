Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True, True)
EndFunction

Function Fragment_Stage_9990_Item_00()
    Actor player = None
    If Alias_Player != None
        player = Alias_Player.GetActorReference()
    EndIf
    If player != None
        player.ModValue(Reputation_AV_Foundation, Rep_Mod_DailyS_Add.GetValue())
        If Caps001 != None
            player.AddItem(Caps001, 60, True)
        EndIf
    EndIf
    SetStage(9995)
EndFunction

Function Fragment_Stage_9992_Item_00()
    Actor player = None
    If Alias_Player != None
        player = Alias_Player.GetActorReference()
    EndIf
    If player != None && W05_Daily_Foundation01_DonationRepValue != None
        player.ModValue(Reputation_AV_Foundation, W05_Daily_Foundation01_DonationRepValue.GetValue())
    EndIf
    SetStage(9995)
EndFunction

Function Fragment_Stage_9998_Item_00()
    Actor player = None
    If Alias_Player != None
        player = Alias_Player.GetActorReference()
    EndIf
    If player != None
        player.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Small.GetValue())
        player.ModValue(Reputation_AV_Foundation, Rep_Mod_Subtract_Small.GetValue())
        If Caps001 != None
            player.AddItem(Caps001, 70, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction
