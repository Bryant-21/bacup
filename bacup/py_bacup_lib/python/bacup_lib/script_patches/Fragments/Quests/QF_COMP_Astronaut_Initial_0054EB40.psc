Function Fragment_Stage_1190_Item_00()
    If COMP_Keyword_QuestStart_Astronaut_Intro_SpawnQuest != None
        COMP_Keyword_QuestStart_Astronaut_Intro_SpawnQuest.SendStoryEventAndWait()
    EndIf
EndFunction

Function Fragment_Stage_1195_Item_00()
    If COMP_Quest_Intro_Astronaut_CrashSpawnQuest != None && COMP_Quest_Intro_Astronaut_CrashSpawnQuest.IsRunning()
        COMP_Quest_Intro_Astronaut_CrashSpawnQuest.SetStage(9000)
    EndIf
EndFunction
