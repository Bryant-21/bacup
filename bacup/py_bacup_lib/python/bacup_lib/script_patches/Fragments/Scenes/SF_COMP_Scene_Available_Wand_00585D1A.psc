Function Fragment_Phase_04_End()
    COMP_InitialQuestScript hireQuest = GetOwningQuest() as COMP_InitialQuestScript
    If hireQuest
        hireQuest.AcquireAlly()
    EndIf
EndFunction
