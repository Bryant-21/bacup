Quests:E01B_Encryptid:QuestScript Function GetEventScript()
    Quest owner = Self as Quest
    Return owner as Quests:E01B_Encryptid:QuestScript
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(5, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True, True)
    If BossRef != None && BossRef.GetReference() != None
        BossRef.GetReference().Enable()
    EndIf
    Quests:E01B_Encryptid:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.SpawnAssaultron()
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(20, True)
    Quests:E01B_Encryptid:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.EnablePylons()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Quests:E01B_Encryptid:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.FinishEvent(True)
    Else
        Stop()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Quests:E01B_Encryptid:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.FinishEvent(False)
    Else
        Stop()
    EndIf
EndFunction

Function Fragment_Stage_9991_Item_00()
    SetObjectiveFailed(5, True)
    Quests:E01B_Encryptid:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.FinishEvent(True)
    Else
        Stop()
    EndIf
EndFunction
