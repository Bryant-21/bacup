Function Fragment_Phase_06_End()
    COMP_InitialQuestScript hireQuest = GetOwningQuest() as COMP_InitialQuestScript
    If hireQuest
        hireQuest.AcquireAlly()
    EndIf
EndFunction
