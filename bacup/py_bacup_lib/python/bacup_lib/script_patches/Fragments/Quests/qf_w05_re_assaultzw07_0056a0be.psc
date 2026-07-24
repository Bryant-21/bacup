Function Fragment_Stage_0020_Item_00()
    REAssaultQuestScript reAssault = Self as REAssaultQuestScript
    If reAssault != None
        reAssault.InitAssault()
        reAssault.StartAssault()
    EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
    REAssaultQuestScript reAssault = Self as REAssaultQuestScript
    If reAssault != None
        reAssault.CompleteAssault()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    REAssaultQuestScript reAssault = Self as REAssaultQuestScript
    If reAssault != None
        reAssault.CompleteAssault()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    REAssaultQuestScript reAssault = Self as REAssaultQuestScript
    If reAssault != None
        reAssault.CleanupAssault()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_01()
    REAssaultQuestScript reAssault = Self as REAssaultQuestScript
    If reAssault != None
        reAssault.CleanupAssault()
    EndIf
EndFunction
