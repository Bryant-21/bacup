Event OnRead()
    Quest bunkerQuest = Game.GetFormFromFile(0x000714FE, "SeventySix.esm") as Quest
    EN01_QuestScript bunkerScript = bunkerQuest as EN01_QuestScript
    If bunkerScript != None
        bunkerScript.HandleImportantNoteRead(GetBaseObject() as Book)
    EndIf
EndEvent
