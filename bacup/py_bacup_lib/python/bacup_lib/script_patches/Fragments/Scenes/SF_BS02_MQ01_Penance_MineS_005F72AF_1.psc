Function Fragment_Phase_02_Begin()
    Quest penanceQuest = Game.GetFormFromFile(0x005F38F1, "SeventySix.esm") as Quest
    Quests:BS02_MQ01_Penance:QuestScript questScript = penanceQuest as Quests:BS02_MQ01_Penance:QuestScript
    If questScript != None
        questScript.PrepareMineScene()
    EndIf
EndFunction
