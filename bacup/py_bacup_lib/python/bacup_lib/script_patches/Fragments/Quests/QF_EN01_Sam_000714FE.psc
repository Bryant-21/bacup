Function Fragment_Stage_0250_Item_00()
    StartEN02MainQuest()
EndFunction

Function Fragment_Stage_0260_Item_00()
    StartEN02MainQuest()
EndFunction

Function StartEN02MainQuest()
    Actor playerRef = Game.GetPlayer()
    If EN02_QuestStartKeyword != None
        EN02_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
    Quest en02MainQuest = Game.GetFormFromFile(0x000293A3, "SeventySix.esm") as Quest
    If en02MainQuest != None && !en02MainQuest.IsCompleted()
        If !en02MainQuest.IsRunning()
            en02MainQuest.Start()
        EndIf
        If !en02MainQuest.IsStageDone(5)
            en02MainQuest.SetStage(5)
        EndIf
    EndIf
EndFunction
