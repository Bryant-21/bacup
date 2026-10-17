Function Fragment_Stage_0010_Item_00()
    ; TODO(recipes): FO76 teaches co_MoM_MistressOfMysteryOutfit at this stage (Server76 2026-10-09). Tales leaves quest-only recipes ungated today; teach them here once recipes are learn-gated.
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
