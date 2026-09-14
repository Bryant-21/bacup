Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 8
        Quest targetQuest = masterScript.MoMQuestList[8].MoMQuest
        Int stage = masterScript.CONST_MoM04_FoundHeadmistressOfficeClue
        If targetQuest != None && !targetQuest.IsStageDone(stage)
            targetQuest.SetStage(stage)
        EndIf
    EndIf
EndFunction
