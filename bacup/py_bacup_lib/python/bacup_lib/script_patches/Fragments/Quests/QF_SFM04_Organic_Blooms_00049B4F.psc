Function Fragment_Stage_1000_Item_00()
    Stop()
EndFunction
Function Fragment_Stage_0010_Item_00()
    Quest bloomQuest = Self
    SFM04_Organic_Blooms_QuestScript bloomScript = bloomQuest as SFM04_Organic_Blooms_QuestScript
    If bloomScript != None
        bloomScript.GrowLocalBlooms()
    EndIf
EndFunction
