Function Fragment_Stage_0190_Item_00()
    EnclaveEventQuestScript eventQuest = (Self as Quest) as EnclaveEventQuestScript
    If eventQuest != None
        eventQuest.ENEvent_RecordCompletion()
    EndIf
EndFunction
