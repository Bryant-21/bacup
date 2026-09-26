Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    Actor playerRef = Game.GetPlayer()
    If masterScript != None && playerRef != None
        MoMItemManagerQuestScript manager = masterScript.MoMItemManager as MoMItemManagerQuestScript
        If manager != None && manager.MoM_Armor_MistressOfMysteryOutfit != None && playerRef.GetItemCount(manager.MoM_Armor_MistressOfMysteryOutfit) == 0
            playerRef.AddItem(manager.MoM_Armor_MistressOfMysteryOutfit, 1)
        EndIf
    EndIf
    SetObjectiveCompleted(10)
    CompleteQuest()
    Stop()
EndFunction
