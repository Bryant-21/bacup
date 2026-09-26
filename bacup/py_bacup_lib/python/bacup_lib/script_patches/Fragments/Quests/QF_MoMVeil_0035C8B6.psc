Function Fragment_Stage_0010_Item_00()
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        masterScript.InitializeLocalEquipmentTracking()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0030_Item_00()
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    Actor playerRef = Game.GetPlayer()
    If masterScript == None || playerRef == None
        Return
    EndIf
    MoMItemManagerQuestScript manager = masterScript.MoMItemManager as MoMItemManagerQuestScript
    If manager == None || manager.MoM_ClothesMistressOfMysteryVeil == None || manager.MoM_ClothesMistressOfMysteryWornVeil == None
        Return
    EndIf
    If playerRef.GetItemCount(manager.MoM_ClothesMistressOfMysteryVeil) == 0
        If playerRef.GetItemCount(manager.MoM_ClothesMistressOfMysteryWornVeil) == 0
            Return
        EndIf
        playerRef.AddItem(manager.MoM_ClothesMistressOfMysteryVeil, 1)
        playerRef.RemoveItem(manager.MoM_ClothesMistressOfMysteryWornVeil, 1, True)
    EndIf
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0099_Item_00()
    SetObjectiveDisplayed(10, False)
    CompleteQuest()
    Stop()
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(10)
    CompleteQuest()
    Stop()
EndFunction
