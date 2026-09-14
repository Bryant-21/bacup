Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 7
        Quest targetQuest = masterScript.MoMQuestList[7].MoMQuest
        Int stage = masterScript.CONST_MoM03_LearnedAboutBrodysKey
        If targetQuest != None && !targetQuest.IsStageDone(stage)
            targetQuest.SetStage(stage)
        EndIf
    EndIf
EndFunction
