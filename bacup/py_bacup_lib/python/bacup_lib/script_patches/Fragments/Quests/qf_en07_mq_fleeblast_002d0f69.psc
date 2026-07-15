Function HandleStage(Int aiStage)
    Quest fleeQuest = Game.GetFormFromFile(0x002D0F69, "SeventySix.esm") as Quest
    EN07_FleeBlastQuestScript fleeScript = fleeQuest as EN07_FleeBlastQuestScript
    If fleeScript != None
        fleeScript.HandleStage(aiStage)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    HandleStage(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    HandleStage(100)
EndFunction
