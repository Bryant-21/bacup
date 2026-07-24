Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200, True, True)
    SetObjectiveDisplayed(2000, True, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(1000, True, True)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1310_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1320_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1330_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1340_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1360_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_5000_Item_00()
    SetObjectiveCompleted(1000, True)
    SetObjectiveDisplayed(5000, True, True)
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetObjectiveCompleted(5000, True)
    SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor player = None
    If Alias_Player != None
        player = Alias_Player.GetActorReference()
    EndIf
    If player != None
        player.ModValue(pReputation_AV_Crater, Rep_Mod_DailyR_Add.GetValue())
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    Stop()
EndFunction
