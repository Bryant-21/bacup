Function Fragment_Phase_06_End()
    CB01_QuestScript trial = GetOwningQuest() as CB01_QuestScript
    If trial != None
        trial.AdvanceTrialPhase()
    EndIf
EndFunction
